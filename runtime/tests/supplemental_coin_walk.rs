//! Rebuild continuation must preserve the actual numbered pack sequence.
#[path = "support/test_admission.rs"]
mod test_admission;

use numera_runtime::{
    atomic,
    coin_rebuild_recovery::RecoveryIo,
    coin_rebuild_walk::{BuildIo, Mutation, Options, walk},
    coin_state,
    telegram::SetState,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

struct Effects {
    path: PathBuf,
    counts: BTreeMap<String, u64>,
    mutations: Vec<Value>,
}
impl RecoveryIo for Effects {
    fn source_exists(&mut self, _: &str) -> bool {
        panic!("no interrupted upload requires recovery")
    }
    fn probe(&mut self, _: &str) -> Result<(SetState, Vec<Value>), String> {
        panic!("no interrupted upload requires recovery")
    }
    fn compare(&mut self, _: &Value, _: &str) -> Result<Option<bool>, String> {
        panic!("no interrupted upload requires recovery")
    }
    fn source_hash(&mut self, _: &str) -> Result<u64, String> {
        panic!("no interrupted upload requires recovery")
    }
    fn live_hash(&mut self, _: &Value) -> Result<u64, String> {
        panic!("no interrupted upload requires recovery")
    }
    fn save(&mut self, state: &Value) -> Result<(), String> {
        atomic::write_json(&self.path, state).map_err(|error| error.to_string())
    }
    fn report(&mut self, _: &str) {}
}
impl BuildIo for Effects {
    fn live_count(&mut self, name: &str) -> Result<u64, String> {
        self.counts
            .get(name)
            .copied()
            .ok_or("unexpected pack".into())
    }
    fn source_status(&mut self, _: &str) -> Result<Option<bool>, String> {
        Ok(Some(false))
    }
    fn delete_old(&mut self, _: &mut Value) -> Result<bool, String> {
        panic!("the prior deletion has already completed")
    }
    fn mutate(&mut self, intent: &Value, _: &str) -> Mutation {
        self.mutations.push(intent.clone());
        let name = intent["set_name"].as_str().unwrap().to_owned();
        if intent["operation"] == "create" {
            assert!(self.counts.insert(name, 1).is_none());
        } else {
            *self.counts.get_mut(&name).unwrap() += 1;
        }
        Mutation::Answered
    }
    fn notify(&mut self, _: &mut Value, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn wait(&mut self) {}
}

fn fixture(indexes: &[u64], last_count: u64, path: PathBuf) -> (Vec<Value>, Value, Effects) {
    let mut counts = BTreeMap::new();
    let sets = indexes
        .iter()
        .enumerate()
        .map(|(position, &index)| {
            let name = format!("fixture{index}_by_bot");
            let count = if position + 1 == indexes.len() {
                last_count
            } else {
                200
            };
            counts.insert(name.clone(), count);
            json!({"index":index,"name":name,"title":format!("Fixture {index}"),"live":count})
        })
        .collect::<Vec<_>>();
    let total = counts.values().sum::<u64>();
    let order = (0..total)
        .map(|position| format!("seed-{position}"))
        .collect::<Vec<_>>();
    let plan = order
        .iter()
        .cloned()
        .chain(["next".into()])
        .map(|key| json!({"rep":key,"kw":key,"tickers":[key]}))
        .collect::<Vec<_>>();
    let state =
        json!({"sets":sets,"order":order,"cursor":total,"in_flight":null,"deleted_old":true});
    assert!(coin_state::problem(&state, Some(&plan)).is_empty());
    atomic::write_json(&path, &state).unwrap();
    (
        plan,
        state,
        Effects {
            path,
            counts,
            mutations: Vec::new(),
        },
    )
}
fn options() -> Options {
    Options {
        owner: 1,
        base: "fixture".into(),
        title: "Fixture".into(),
        bot: "bot".into(),
    }
}
fn disk(io: &Effects) -> Value {
    serde_json::from_slice(&std::fs::read(&io.path).unwrap()).unwrap()
}

#[test]
fn full_recorded_pack_continues_from_its_actual_index() {
    for indexes in [vec![1], vec![7], vec![1, 7]] {
        let root = tempfile::tempdir().unwrap();
        let (plan, mut state, mut io) = fixture(&indexes, 200, root.path().join("state.json"));
        walk(&plan, &mut state, &options(), &mut io).unwrap();
        let expected = indexes.last().unwrap() + 1;
        assert_eq!(io.mutations.len(), 1);
        assert_eq!(io.mutations[0]["operation"], "create");
        assert_eq!(io.mutations[0]["set_index"], expected);
        assert_eq!(
            io.mutations[0]["set_name"],
            format!("fixture{expected}_by_bot")
        );
        assert!(coin_state::problem(&state, Some(&plan)).is_empty());
        assert_eq!(disk(&io), state);
        walk(&plan, &mut state, &options(), &mut io).unwrap();
        assert_eq!(
            io.mutations.len(),
            1,
            "completed resume must not upload again"
        );
    }
}

#[test]
fn maximum_pack_index_refuses_creation_without_saving_an_intent() {
    let root = tempfile::tempdir().unwrap();
    let (plan, mut state, mut io) = fixture(&[u64::MAX], 200, root.path().join("state.json"));
    let state_before = state.clone();
    let before = std::fs::read(&io.path).unwrap();
    let result = walk(&plan, &mut state, &options(), &mut io);
    assert!(
        result
            .as_ref()
            .is_err_and(|error| error.contains("index overflow")),
        "{result:?}"
    );
    assert!(io.mutations.is_empty());
    assert_eq!(
        state, state_before,
        "refusal must not advance the next cursor"
    );
    assert_eq!(std::fs::read(&io.path).unwrap(), before);
}

#[test]
fn first_pack_starts_at_one_and_partial_last_pack_keeps_its_index() {
    for (indexes, count, operation, expected) in
        [(vec![], 0, "create", 1), (vec![7], 199, "add", 7)]
    {
        let root = tempfile::tempdir().unwrap();
        let (plan, mut state, mut io) = fixture(&indexes, count, root.path().join("state.json"));
        walk(&plan, &mut state, &options(), &mut io).unwrap();
        assert_eq!(io.mutations.len(), 1);
        assert_eq!(io.mutations[0]["operation"], operation);
        assert_eq!(io.mutations[0]["set_index"], expected);
        assert!(coin_state::problem(&state, Some(&plan)).is_empty());
        assert_eq!(disk(&io), state);
    }
}
