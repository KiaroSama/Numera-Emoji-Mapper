//! Rust syntax policy for the production mutators, with live lexical lease scopes.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use syn::{
    Expr, Pat, Stmt,
    visit::{self, Visit},
};
pub const MAP: &str = "map";
pub const PACK: &str = "pack";
#[derive(Default, Debug)]
pub struct Function {
    pub name: String,
    pub pairs: Vec<(String, String)>,
    pub acquired: BTreeSet<String>,
    pub calls: BTreeSet<String>,
}
fn expression_kind(expr: &Expr, bindings: &BTreeMap<String, String>) -> Option<String> {
    struct Text(Vec<String>);
    impl<'a> Visit<'a> for Text {
        fn visit_lit_str(&mut self, s: &'a syn::LitStr) {
            self.0.push(s.value())
        }
        fn visit_expr_path(&mut self, p: &'a syn::ExprPath) {
            if let Some(s) = p.path.segments.last() {
                self.0.push(s.ident.to_string());
            }
            visit::visit_expr_path(self, p)
        }
    }
    let mut t = Text(vec![]);
    t.visit_expr(expr);
    if t.0
        .iter()
        .any(|s| s.contains("canonical_map.lock") || s == "canonical_map_path")
    {
        Some(MAP.into())
    } else if t.0.iter().any(|s| {
        s.contains("pack_")
            || s == "pack_family_path"
            || s.contains("publish_") && s.contains(".lock")
    }) {
        Some(PACK.into())
    } else {
        t.0.iter().find_map(|s| bindings.get(s).cloned())
    }
}
fn acquisition(expr: &Expr, bindings: &BTreeMap<String, String>) -> Option<String> {
    match expr {
        Expr::Call(c) => {
            let name = if let Expr::Path(p) = c.func.as_ref() {
                p.path.segments.last()?.ident.to_string()
            } else {
                return None;
            };
            if ["acquire", "canonical_map_lock", "exclusive_lock"].contains(&name.as_str()) {
                if name == "canonical_map_lock" {
                    return Some(MAP.into());
                }
                if name == "exclusive_lock" {
                    return Some(PACK.into());
                }
                c.args.first().and_then(|e| expression_kind(e, bindings))
            } else {
                None
            }
        }
        Expr::Try(t) => acquisition(&t.expr, bindings),
        Expr::MethodCall(c) => acquisition(&c.receiver, bindings),
        Expr::Match(m) => acquisition(&m.expr, bindings),
        _ => None,
    }
}
fn block(b: &syn::Block, held: &[String], bindings: &BTreeMap<String, String>, f: &mut Function) {
    let mut held = held.to_vec();
    let mut bindings = bindings.clone();
    for stmt in &b.stmts {
        let expr = match stmt {
            Stmt::Local(l) => l.init.as_ref().map(|i| i.expr.as_ref()),
            Stmt::Expr(e, _) => Some(e),
            _ => None,
        };
        if let Some(expr) = expr {
            if let Some(k) = acquisition(expr, &bindings) {
                for outer in &held {
                    f.pairs.push((outer.clone(), k.clone()));
                }
                f.acquired.insert(k.clone());
                held.push(k);
            }
            if let Stmt::Local(l) = stmt
                && let Pat::Ident(i) = &l.pat
                && let Some(k) = expression_kind(expr, &bindings)
            {
                bindings.insert(i.ident.to_string(), k);
            }
            struct Nested<'a> {
                held: &'a [String],
                bindings: &'a BTreeMap<String, String>,
                f: &'a mut Function,
            }
            impl<'s> Visit<'s> for Nested<'_> {
                fn visit_block(&mut self, b: &'s syn::Block) {
                    block(b, self.held, self.bindings, self.f)
                }
                fn visit_item_fn(&mut self, _: &'s syn::ItemFn) {}
                fn visit_expr_call(&mut self, c: &'s syn::ExprCall) {
                    if let Expr::Path(p) = c.func.as_ref()
                        && let Some(n) = p.path.segments.last()
                    {
                        self.f.calls.insert(n.ident.to_string());
                    }
                    visit::visit_expr_call(self, c)
                }
            }
            Nested {
                held: &held,
                bindings: &bindings,
                f,
            }
            .visit_expr(expr);
        }
    }
}
pub fn functions(source: &str) -> Vec<Function> {
    let tree = syn::parse_file(source).unwrap();
    struct Fs(Vec<Function>);
    impl<'a> Visit<'a> for Fs {
        fn visit_item_fn(&mut self, n: &'a syn::ItemFn) {
            if n.attrs.iter().any(|a| a.path().is_ident("test")) {
                return;
            }
            let mut f = Function {
                name: n.sig.ident.to_string(),
                ..Function::default()
            };
            block(&n.block, &[], &BTreeMap::new(), &mut f);
            self.0.push(f);
        }
        fn visit_impl_item_fn(&mut self, n: &'a syn::ImplItemFn) {
            let mut f = Function {
                name: n.sig.ident.to_string(),
                ..Function::default()
            };
            block(&n.block, &[], &BTreeMap::new(), &mut f);
            self.0.push(f);
        }
        fn visit_item_mod(&mut self, m: &'a syn::ItemMod) {
            if m.attrs.iter().any(|a| a.path().is_ident("cfg")) {
                return;
            }
            visit::visit_item_mod(self, m)
        }
    }
    let mut f = Fs(vec![]);
    f.visit_file(&tree);
    f.0
}
pub fn sources() -> BTreeMap<String, String> {
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR"));
    let library = runtime.join("src/lib.rs");
    let tree = syn::parse_file(&std::fs::read_to_string(&library).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for item in tree.items {
        let syn::Item::Mod(module) = item else {
            continue;
        };
        if module.attrs.iter().any(|a| a.path().is_ident("cfg")) {
            continue;
        }
        let name = module.ident.to_string();
        let explicit = module.attrs.iter().find_map(|a| {
            if !a.path().is_ident("path") {
                return None;
            }
            let syn::Meta::NameValue(meta) = &a.meta else {
                return None;
            };
            let Expr::Lit(value) = &meta.value else {
                return None;
            };
            let syn::Lit::Str(path) = &value.lit else {
                return None;
            };
            Some(runtime.join("src").join(path.value()))
        });
        let path = explicit.unwrap_or_else(|| runtime.join("src").join(format!("{name}.rs")));
        assert!(
            path.is_file(),
            "compiled production module absent: {}",
            path.display()
        );
        assert!(
            out.insert(name, std::fs::read_to_string(path).unwrap())
                .is_none()
        );
    }
    out
}
pub fn helper_clashes(source: &str) -> Vec<String> {
    let fs = functions(source);
    let takes = fs
        .iter()
        .map(|f| (&f.name, &f.acquired))
        .collect::<BTreeMap<_, _>>();
    let mut bad = vec![];
    for f in &fs {
        for callee in f.calls.iter().filter(|s| *s != &f.name) {
            if let Some(other) = takes.get(callee)
                && !f.acquired.is_disjoint(other)
            {
                bad.push(format!(
                    "{} holds {:?} and calls {callee} holding {:?}",
                    f.name, f.acquired, other
                ));
            }
        }
    }
    bad
}
pub fn call_paths(source: &str) -> BTreeSet<Vec<String>> {
    struct Calls(BTreeSet<Vec<String>>);
    impl<'a> Visit<'a> for Calls {
        fn visit_expr_call(&mut self, c: &'a syn::ExprCall) {
            if let Expr::Path(p) = c.func.as_ref() {
                self.0.insert(
                    p.path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect(),
                );
            }
            visit::visit_expr_call(self, c)
        }
    }
    let mut c = Calls(BTreeSet::new());
    c.visit_file(&syn::parse_file(source).unwrap());
    c.0
}

pub fn string_literals(source: &str) -> BTreeSet<String> {
    struct Literals(BTreeSet<String>);
    impl<'a> Visit<'a> for Literals {
        fn visit_lit_str(&mut self, l: &'a syn::LitStr) {
            self.0.insert(l.value());
        }
    }
    let mut values = Literals(BTreeSet::new());
    values.visit_file(&syn::parse_file(source).unwrap());
    values.0
}
