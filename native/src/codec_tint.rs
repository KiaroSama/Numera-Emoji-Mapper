//! Lottie recolouring over external JSON objects; alpha and gradient opacity remain untouched.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyList};

pub fn parse_tint(text: &str) -> Result<[u8; 3], String> {
    // CPython also strips these four ASCII record separators, unlike Rust's whitespace set.
    let value = text
        .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .trim_start_matches('#');
    if value.len() != 6 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "tint must be #RRGGBB, got {}",
            crate::python_repr::quoted(text)
        ));
    }
    let mut rgb = [0; 3];
    for (i, channel) in rgb.iter_mut().enumerate() {
        *channel = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .expect("six validated ASCII hex digits");
    }
    Ok(rgb)
}

pub fn numeric(value: &Bound<'_, PyAny>) -> bool {
    value.cast::<PyList>().is_ok_and(|values| {
        values
            .iter()
            .all(|value| value.is_instance_of::<PyInt>() || value.is_instance_of::<PyFloat>())
    })
}

fn recolour(values: &Bound<'_, PyList>, rgb: [f64; 3]) -> PyResult<()> {
    for (index, colour) in rgb.into_iter().take(values.len()).enumerate() {
        values.set_item(index, colour)?;
    }
    Ok(())
}

pub fn tint_gradient(gradient: &Bound<'_, PyAny>, rgb: [f64; 3]) -> PyResult<()> {
    let Ok(gradient) = gradient.cast::<PyDict>() else {
        return Ok(());
    };
    let Some(key) = gradient.get_item("k")? else {
        return Ok(());
    };
    let Ok(key) = key.cast::<PyDict>() else {
        return Ok(());
    };
    let Some(stops) = key.get_item("k")? else {
        return Ok(());
    };
    if !numeric(&stops) {
        return Ok(());
    }
    let stops = stops.cast::<PyList>()?;
    let length = stops.len();
    let count = match gradient.get_item("p")? {
        Some(value) if value.is_instance_of::<PyInt>() && value.ge(0)? => {
            if value.ge(length / 4)? {
                length / 4
            } else {
                value.extract::<usize>()?
            }
        }
        _ if length.is_multiple_of(4) => length / 4,
        _ => 0,
    };
    for base in (0..count * 4).step_by(4) {
        for (index, colour) in rgb.into_iter().enumerate() {
            stops.set_item(base + 1 + index, colour)?;
        }
    }
    Ok(())
}

struct Recursion<'py> {
    _py: Python<'py>,
}
#[allow(unsafe_code)]
impl<'py> Recursion<'py> {
    fn enter(py: Python<'py>) -> PyResult<Self> {
        // SAFETY: the token holds the interpreter attachment; the message is a static C string.
        if unsafe { pyo3::ffi::Py_EnterRecursiveCall(c" while tinting Lottie".as_ptr()) } != 0 {
            return Err(PyErr::fetch(py));
        }
        Ok(Self { _py: py })
    }
}
#[allow(unsafe_code)]
impl Drop for Recursion<'_> {
    fn drop(&mut self) {
        // SAFETY: this guard owns exactly one successful entry and cannot outlive its attachment.
        unsafe { pyo3::ffi::Py_LeaveRecursiveCall() };
    }
}

pub fn tint_lottie(node: &Bound<'_, PyAny>, rgb: [f64; 3]) -> PyResult<()> {
    let _recursion = Recursion::enter(node.py())?;
    if let Ok(object) = node.cast::<PyDict>() {
        if let Some(colour) = object.get_item("c")?
            && let Ok(colour) = colour.cast::<PyDict>()
            && let Some(values) = colour.get_item("k")?
        {
            if numeric(&values) {
                recolour(values.cast::<PyList>()?, rgb)?;
            } else if let Ok(frames) = values.cast::<PyList>() {
                for frame in frames.iter() {
                    if let Ok(frame) = frame.cast::<PyDict>() {
                        for field in ["s", "e"] {
                            if let Some(value) = frame.get_item(field)?
                                && numeric(&value)
                            {
                                recolour(value.cast::<PyList>()?, rgb)?;
                            }
                        }
                    }
                }
            }
        }
        if let Some(gradient) = object.get_item("g")? {
            tint_gradient(&gradient, rgb)?;
        }
        for (_, value) in object.iter() {
            tint_lottie(&value, rgb)?;
        }
    } else if let Ok(array) = node.cast::<PyList>() {
        for value in array.iter() {
            tint_lottie(&value, rgb)?;
        }
    }
    Ok(())
}
