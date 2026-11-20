use crate::ports::PackContents;

pub const VANILLA_PACK: &str = "vanilla";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DataPackLists {
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackSource {
    BuiltIn,
    World,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInfo {
    pub id: String,
    pub description: String,
    pub source: PackSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    First,
    Last,
    Before(String),
    After(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackProblem {
    Unknown(String),
    AlreadyEnabled(String),
    NotEnabled(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packs {
    available: Vec<PackInfo>,
    enabled: Vec<String>,
    foreign: DataPackLists,
}

impl Default for Packs {
    fn default() -> Self {
        Self::discover(&[], None)
    }
}

pub fn folder_pack_id(name: &str) -> String {
    format!("file/{name}")
}

impl Packs {
    pub fn discover(contents: &[PackContents], lists: Option<&DataPackLists>) -> Self {
        let mut available = vec![PackInfo {
            id: VANILLA_PACK.to_owned(),
            description: String::new(),
            source: PackSource::BuiltIn,
        }];
        for pack in contents {
            available.push(PackInfo {
                id: folder_pack_id(&pack.name),
                description: pack.meta.description.clone(),
                source: PackSource::World,
            });
        }
        let known = |id: &str| available.iter().any(|pack| pack.id == id);
        let mut enabled = vec![VANILLA_PACK.to_owned()];
        if let Some(lists) = lists {
            for id in &lists.enabled {
                if known(id) && !enabled.contains(id) {
                    enabled.push(id.clone());
                }
            }
        }
        for pack in &available {
            let listed = lists.is_some_and(|lists| lists.disabled.contains(&pack.id));
            if !listed && !enabled.contains(&pack.id) {
                enabled.push(pack.id.clone());
            }
        }
        let foreign = lists.map_or_else(DataPackLists::default, |lists| {
            let foreign = |names: &[String]| {
                names
                    .iter()
                    .filter(|id| !id.starts_with("file/") && !known(id))
                    .cloned()
                    .collect()
            };
            DataPackLists {
                enabled: foreign(&lists.enabled),
                disabled: foreign(&lists.disabled),
            }
        });
        Self {
            available,
            enabled,
            foreign,
        }
    }

    pub fn rediscover(&self, contents: &[PackContents]) -> Self {
        let lists = DataPackLists {
            enabled: self
                .enabled
                .iter()
                .chain(&self.foreign.enabled)
                .cloned()
                .collect(),
            disabled: contents
                .iter()
                .map(|pack| folder_pack_id(&pack.name))
                .filter(|id| !self.is_enabled(id))
                .chain(self.foreign.disabled.iter().cloned())
                .collect(),
        };
        Self::discover(contents, Some(&lists))
    }

    pub fn available(&self) -> &[PackInfo] {
        &self.available
    }

    pub fn enabled(&self) -> &[String] {
        &self.enabled
    }

    pub fn info(&self, id: &str) -> Option<&PackInfo> {
        self.available.iter().find(|pack| pack.id == id)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled.iter().any(|enabled| enabled == id)
    }

    pub fn disabled(&self) -> impl Iterator<Item = &PackInfo> {
        self.available
            .iter()
            .filter(|pack| !self.is_enabled(&pack.id))
    }

    pub fn lists(&self) -> DataPackLists {
        DataPackLists {
            enabled: self
                .enabled
                .iter()
                .chain(&self.foreign.enabled)
                .cloned()
                .collect(),
            disabled: self
                .disabled()
                .map(|pack| pack.id.clone())
                .chain(self.foreign.disabled.iter().cloned())
                .collect(),
        }
    }

    pub fn enable(&mut self, id: &str, placement: &Placement) -> Result<(), PackProblem> {
        if self.info(id).is_none() {
            return Err(PackProblem::Unknown(id.to_owned()));
        }
        if self.is_enabled(id) {
            return Err(PackProblem::AlreadyEnabled(id.to_owned()));
        }
        let index = match placement {
            Placement::First => 0,
            Placement::Last => self.enabled.len(),
            Placement::Before(other) => self.position_of(other)?,
            Placement::After(other) => self.position_of(other)? + 1,
        };
        self.enabled.insert(index, id.to_owned());
        Ok(())
    }

    pub fn disable(&mut self, id: &str) -> Result<(), PackProblem> {
        if self.info(id).is_none() {
            return Err(PackProblem::Unknown(id.to_owned()));
        }
        if id == VANILLA_PACK || !self.is_enabled(id) {
            return Err(PackProblem::NotEnabled(id.to_owned()));
        }
        self.enabled.retain(|enabled| enabled != id);
        Ok(())
    }

    fn position_of(&self, id: &str) -> Result<usize, PackProblem> {
        self.enabled
            .iter()
            .position(|enabled| enabled == id)
            .ok_or_else(|| PackProblem::Unknown(id.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::PackMeta;

    fn pack(name: &str) -> PackContents {
        PackContents {
            name: name.to_owned(),
            meta: PackMeta::default(),
            functions: Vec::new(),
            function_tags: Vec::new(),
            ..PackContents::default()
        }
    }

    fn ids(packs: &Packs) -> Vec<&str> {
        packs.enabled().iter().map(String::as_str).collect()
    }

    #[test]
    fn new_folders_are_enabled_after_vanilla_in_order() {
        let packs = Packs::discover(&[pack("a"), pack("b")], None);
        assert_eq!(ids(&packs), ["vanilla", "file/a", "file/b"]);
        assert_eq!(packs.lists().disabled, Vec::<String>::new());
    }

    #[test]
    fn the_saved_lists_decide_enabled_order_and_disabled_packs() {
        let lists = DataPackLists {
            enabled: vec![
                "vanilla".to_owned(),
                "file/b".to_owned(),
                "file/gone".to_owned(),
            ],
            disabled: vec!["file/a".to_owned()],
        };
        let packs = Packs::discover(&[pack("a"), pack("b"), pack("c")], Some(&lists));
        assert_eq!(ids(&packs), ["vanilla", "file/b", "file/c"]);
        assert_eq!(packs.lists().disabled, ["file/a"]);
    }

    #[test]
    fn built_in_pack_ids_this_server_does_not_know_stay_in_the_lists() {
        let lists = DataPackLists {
            enabled: vec!["vanilla".to_owned(), "minecraft:trade_rebalance".to_owned()],
            disabled: vec![
                "minecraft:redstone_experiments".to_owned(),
                "file/gone".to_owned(),
            ],
        };
        let packs = Packs::discover(&[pack("a")], Some(&lists));
        let kept = packs.lists();
        assert_eq!(
            kept.enabled,
            ["vanilla", "file/a", "minecraft:trade_rebalance"]
        );
        assert_eq!(kept.disabled, ["minecraft:redstone_experiments"]);
        let again = packs.rediscover(&[pack("a")]).lists();
        assert_eq!(again, kept);
    }

    #[test]
    fn enable_places_a_pack_and_disable_removes_it() {
        let lists = DataPackLists {
            enabled: vec!["vanilla".to_owned()],
            disabled: vec![
                "file/a".to_owned(),
                "file/b".to_owned(),
                "file/c".to_owned(),
            ],
        };
        let mut packs = Packs::discover(&[pack("a"), pack("b"), pack("c")], Some(&lists));
        packs.enable("file/a", &Placement::Last).unwrap();
        packs.enable("file/b", &Placement::First).unwrap();
        packs
            .enable("file/c", &Placement::Before("file/a".to_owned()))
            .unwrap();
        assert_eq!(ids(&packs), ["file/b", "vanilla", "file/c", "file/a"]);
        assert_eq!(
            packs.enable("file/a", &Placement::Last),
            Err(PackProblem::AlreadyEnabled("file/a".to_owned()))
        );
        packs.disable("file/c").unwrap();
        assert_eq!(ids(&packs), ["file/b", "vanilla", "file/a"]);
        assert_eq!(packs.lists().disabled, ["file/c"]);
        assert_eq!(
            packs.disable("file/c"),
            Err(PackProblem::NotEnabled("file/c".to_owned()))
        );
        assert_eq!(
            packs.enable("file/zzz", &Placement::Last),
            Err(PackProblem::Unknown("file/zzz".to_owned()))
        );
        assert_eq!(
            packs.disable("vanilla"),
            Err(PackProblem::NotEnabled("vanilla".to_owned()))
        );
    }
}
