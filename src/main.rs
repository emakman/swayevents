mod output;

const CONFIG_SUBDIR: &str = "swayevents";
const CONFIG_FILE: &str = "config.toml";

use futures_util::stream::StreamExt;
use swayipc_async::{
    Connection as SwayIpc, Event as SwayEvent, EventType as SwayEventType, Fallible,
};

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Triggers {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    on_output_added: Vec<output::OnOutputAdded>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    on_output_removed: Vec<output::OnOutputRemoved>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    on_output_changed: Vec<output::OnOutputChanged>,
}
impl Triggers {
    fn load(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(o) => match toml::from_str(&o) {
                Ok(o) => o,
                Err(e) => {
                    panic!("Ill-formed config: {e}");
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut me = Self::default();
                me.on_output_added.push(output::OnOutputAdded {
                    display: None,
                    exec: "HI".into(),
                    args: vec![],
                });
                me.on_output_added.push(output::OnOutputAdded {
                    display: None,
                    exec: "HI".into(),
                    args: vec![],
                });
                me.save();
                eprintln!("Created empty config file {}.", path.display());
                me
            }
            Err(e) => {
                panic!("{e:?}")
            }
        }
    }
    fn save(&self) {
        let data = toml::to_string(self).unwrap();
        let Some(mut config) = dirs::config_dir() else {
            panic!("Could not determine config directory.");
        };
        if !config.exists() {
            panic!("Could not determine config directory.");
        }
        config.push(CONFIG_SUBDIR);
        if !config.exists() {
            if let Err(e) = std::fs::create_dir(&config) {
                panic!("Could not create config directory: {e}");
            }
        }
        config.push(CONFIG_FILE);
        if let Err(e) = std::fs::write(&config, data) {
            panic!("Could not write config file: {e}");
        }
    }
    async fn exec(&mut self, path: &std::path::Path, ev: Event) {
        match ev {
            Event::Output(output::OutputChange::Added(o)) => {
                println!("Added: {}", o.name);
                for t in &self.on_output_added {
                    t.exec(&o).await
                }
            }
            Event::Output(output::OutputChange::Removed(o)) => {
                println!("Removed: {}", o.name);
                for t in &self.on_output_removed {
                    t.exec(&o).await
                }
            }
            Event::Output(output::OutputChange::Changed(o, d)) => {
                println!("Changed: {}", o.name);
                for t in &self.on_output_changed {
                    t.exec(&o, &d).await
                }
            }
            Event::ConfigUpdate => *self = Self::load(path),
        }
    }
}

#[derive(Debug)]
enum Event {
    Output(output::OutputChange),
    ConfigUpdate,
}
impl Event {
    async fn from_sway(sway: &mut SwayState, ev: SwayEvent) -> Fallible<Vec<Self>> {
        match ev {
            SwayEvent::Output(swayipc_async::OutputEvent { change, .. }) => match change {
                swayipc_async::OutputChange::Unspecified => sway
                    .outputs
                    .update(&mut sway.ipc)
                    .await
                    .map(|v| v.into_iter().map(Event::Output).collect()),
                _ => unimplemented!("Wait, that wasn't exhaustive!"),
            },
            _ => unreachable!("Changed the subscription list without adding more cases!"),
        }
    }
}

struct SwayState {
    ipc: SwayIpc,
    outputs: output::Outputs,
}
impl SwayState {
    async fn new() -> Fallible<Self> {
        let mut ipc = SwayIpc::new().await?;
        let outputs = output::Outputs::new(&mut ipc).await?;
        Ok(Self { ipc, outputs })
    }
}

fn run_cmd(exec: &str, args: &[String], stdin: std::process::Stdio) -> std::process::Child {
    std::process::Command::new(exec)
        .args(args)
        .stdin(stdin)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
}

#[tokio::main]
async fn main() {
    let subs = [SwayEventType::Output];
    let file = dirs::config_dir()
        .map(|mut d| {
            d.push(CONFIG_SUBDIR);
            d.push(CONFIG_FILE);
            d
        })
        .expect("Cannot determine correct location for config file");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut triggers = Triggers::load(&file);

    {
        let file = file.clone();
        tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                triggers.exec(&file, ev).await;
            }
        });
    }

    use notify::Watcher;
    let mut watcher = notify::RecommendedWatcher::new(
        {
            let tx = tx.clone();
            move |_| tx.send(Event::ConfigUpdate).unwrap()
        },
        notify::Config::default(),
    )
    .unwrap();
    watcher
        .watch(&file, notify::RecursiveMode::NonRecursive)
        .unwrap();

    let mut state = SwayState::new().await.unwrap();
    let mut events = SwayIpc::new().await.unwrap().subscribe(subs).await.unwrap();

    while let Some(event) = events.next().await {
        for ev in Event::from_sway(&mut state, event.unwrap()).await.unwrap() {
            tx.send(ev).unwrap();
        }
    }
}
