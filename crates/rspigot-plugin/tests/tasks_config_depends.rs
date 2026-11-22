mod fake_host;

use std::time::Duration;

use fake_host::*;
use rspigot_plugin::{FromConfig, Plugin, Server, config, listeners, run_with};
use tokio::sync::mpsc;

struct Beats {
    seen: mpsc::UnboundedSender<&'static str>,
}

#[listeners]
impl Beats {
    #[task(every = "100ms")]
    async fn beat(&self) {
        let _ = self.seen.send("beat");
    }

    #[task(every = "100ms")]
    async fn beat_with_server(&self, server: &Server) {
        let _ = server;
        let _ = self.seen.send("beat-with-server");
    }
}

#[derive(Plugin)]
#[plugin(
    name = "ticker",
    version = "1.0.0",
    depends = ["base", "economy"],
    soft_depends = ["extra"],
    load_before = ["late"]
)]
struct Ticker {
    #[module]
    beats: Beats,
}

#[tokio::test]
async fn a_hello_carries_the_declared_relations() {
    let mut host = Host::start().await;
    let (seen, _) = mpsc::unbounded_channel();
    tokio::spawn(run_with(
        Ticker {
            beats: Beats { seen },
        },
        host.url.clone(),
        None,
    ));
    let hello = host.knock().await.hello;
    assert_eq!(hello.depends, ["base", "economy"]);
    assert_eq!(hello.soft_depends, ["extra"]);
    assert_eq!(hello.load_before, ["late"]);
}

#[tokio::test]
async fn a_plugin_without_relations_declares_none() {
    #[derive(Plugin)]
    #[plugin(name = "plain")]
    struct Plain {
        #[module]
        beats: Beats,
    }
    let mut host = Host::start().await;
    let (seen, _) = mpsc::unbounded_channel();
    tokio::spawn(run_with(
        Plain {
            beats: Beats { seen },
        },
        host.url.clone(),
        None,
    ));
    let hello = host.knock().await.hello;
    assert!(hello.depends.is_empty());
    assert!(hello.soft_depends.is_empty());
    assert!(hello.load_before.is_empty());
}

#[tokio::test]
async fn tasks_of_a_module_run_on_the_plugin_runtime_while_it_is_connected() {
    let mut host = Host::start().await;
    let (seen, mut beats) = mpsc::unbounded_channel();
    tokio::spawn(run_with(
        Ticker {
            beats: Beats { seen },
        },
        host.url.clone(),
        None,
    ));
    let _session = host.knock().await.accept();
    let mut kinds = Vec::new();
    for _ in 0..4 {
        kinds.push(within(beats.recv()).await.unwrap());
    }
    assert!(kinds.contains(&"beat"));
    assert!(kinds.contains(&"beat-with-server"));
}

#[tokio::test]
async fn tasks_do_not_run_before_the_plugin_is_accepted() {
    let mut host = Host::start().await;
    let (seen, mut beats) = mpsc::unbounded_channel();
    tokio::spawn(run_with(
        Ticker {
            beats: Beats { seen },
        },
        host.url.clone(),
        None,
    ));
    let _knock = host.knock().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(beats.try_recv().is_err());
}

#[config]
#[derive(Debug, PartialEq)]
struct Settings {
    greeting: String,
    limit: u16,
    ratio: f64,
    names: Vec<String>,
    motd: Option<String>,
    nested: bool,
}

#[test]
fn a_config_struct_loads_from_the_plugin_folder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "greeting = \"hi\"\nlimit = 7\nratio = 2\nnames = [\"a\", \"b\"]\nnested = true\n",
    )
    .unwrap();
    let settings = Settings::load_from(dir.path()).unwrap();
    assert_eq!(
        settings,
        Settings {
            greeting: "hi".to_owned(),
            limit: 7,
            ratio: 2.0,
            names: vec!["a".to_owned(), "b".to_owned()],
            motd: None,
            nested: true,
        }
    );
}

#[test]
fn a_config_struct_refuses_a_missing_or_mistyped_key() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "greeting = \"hi\"\n").unwrap();
    let error = Settings::load_from(dir.path()).unwrap_err();
    assert!(error.to_string().contains("`limit` is missing"), "{error}");
    std::fs::write(
        dir.path().join("config.toml"),
        "greeting = 3\nlimit = 1\nratio = 1.0\nnames = []\nnested = false\n",
    )
    .unwrap();
    assert!(Settings::load_from(dir.path()).is_err());
    let config = rspigot_plugin::Config::open(dir.path()).unwrap();
    assert!(Settings::from_config(&config).is_err());
}
