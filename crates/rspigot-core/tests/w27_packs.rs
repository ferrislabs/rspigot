mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Link, calm_server};
use rspigot_core::commands::{CommandOutcome, CommandSender, execute};
use rspigot_core::ports::{DatapackError, DatapackSource, PackContents, PackFile, PackMeta};
use rspigot_core::server::Server;
use rspigot_domain::Identifier;

#[derive(Default)]
struct Packs {
    created: Mutex<Vec<(String, String)>>,
}

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(vec![PackContents {
            name: "main".to_owned(),
            meta: PackMeta::default(),
            functions: vec![PackFile {
                id: Identifier::parse("t:f").expect("id"),
                text: "say one\nsay two\n".to_owned(),
            }],
            ..PackContents::default()
        }])
    }

    fn create_pack(&self, name: &str, meta: &str) -> Result<bool, DatapackError> {
        let mut created = self.created.lock().expect("created");
        if created.iter().any(|(existing, _)| existing == name) {
            return Ok(false);
        }
        created.push((name.to_owned(), meta.to_owned()));
        Ok(true)
    }
}

fn started() -> (Server<Link>, Arc<Packs>) {
    let packs = Arc::new(Packs::default());
    let mut server = calm_server().with_datapacks(Arc::clone(&packs));
    server.start(Duration::from_secs(10));
    (server, packs)
}

#[test]
fn w27_datapack_create_writes_a_pack_with_its_description() {
    let (mut server, packs) = started();
    let outcome = execute(
        &mut server,
        CommandSender::Console,
        "datapack create mine {text:'My pack',bold:true}",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    let created = packs.created.lock().expect("created").clone();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].0, "mine");
    let meta: serde_json::Value = serde_json::from_str(&created[0].1).expect("json");
    assert_eq!(meta["pack"]["description"]["text"], "My pack");
    assert_eq!(meta["pack"]["description"]["bold"], true);
    assert_eq!(meta["pack"]["min_format"], serde_json::json!([121, 0]));
    let again = execute(
        &mut server,
        CommandSender::Console,
        "datapack create mine {text:'x'}",
    );
    assert_eq!(again, CommandOutcome::Failure);
    let bad = execute(
        &mut server,
        CommandSender::Console,
        "datapack create ../escape {text:'x'}",
    );
    assert_eq!(bad, CommandOutcome::Failure);
    assert_eq!(packs.created.lock().expect("created").len(), 1);
}

#[test]
fn w27_debug_function_runs_the_function_and_writes_a_trace() {
    let (mut server, _packs) = started();
    let outcome = execute(&mut server, CommandSender::Console, "debug function t:f");
    assert_eq!(outcome, CommandOutcome::Success);
    let traces: Vec<_> = std::fs::read_dir("debug")
        .expect("the debug folder")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("commands-"))
        .collect();
    assert!(!traces.is_empty());
    let text = std::fs::read_to_string(traces[0].path()).expect("trace");
    assert!(text.contains("# t:f") && text.contains("say two"));
    for entry in traces {
        let _ = std::fs::remove_file(entry.path());
    }
    let _ = std::fs::remove_dir("debug");
    let missing = execute(
        &mut server,
        CommandSender::Console,
        "debug function t:missing",
    );
    assert_eq!(missing, CommandOutcome::Failure);
}
