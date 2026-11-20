use super::library::FunctionLibrary;
use super::packs::{DataPackLists, Packs, folder_pack_id};
use super::schedule::ScheduledEvents;
use crate::pack_data::PackData;
use crate::ports::PackContents;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Returned {
    Value(i32),
    Failure,
}

pub const MAX_NESTING: usize = 128;
pub const DEFAULT_COMMAND_LIMIT: u64 = 65_536;

#[derive(Debug, Clone)]
pub struct FunctionRuntime {
    pub depth: usize,
    pub executed: u64,
    pub limit: u64,
    pub aborted: bool,
    pub silent: bool,
    pub permission_cap: Option<u8>,
    pub returned: Option<Returned>,
}

impl Default for FunctionRuntime {
    fn default() -> Self {
        Self {
            depth: 0,
            executed: 0,
            limit: DEFAULT_COMMAND_LIMIT,
            aborted: false,
            silent: false,
            permission_cap: None,
            returned: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FunctionState {
    pub library: FunctionLibrary,
    pub packs: Packs,
    pub scheduled: ScheduledEvents,
    pub runtime: FunctionRuntime,
    pub data: PackData,
    contents: Vec<PackContents>,
}

impl FunctionState {
    pub fn load(contents: Vec<PackContents>, lists: Option<&DataPackLists>) -> Self {
        let packs = Packs::discover(&contents, lists);
        let mut state = Self {
            packs,
            contents,
            ..Self::default()
        };
        state.rebuild();
        state
    }

    pub fn reload(&mut self, contents: Vec<PackContents>) {
        self.packs = self.packs.rediscover(&contents);
        self.contents = contents;
        self.rebuild();
    }

    pub fn apply(&mut self, packs: Packs) {
        self.packs = packs;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let enabled: Vec<&PackContents> = self
            .packs
            .enabled()
            .iter()
            .filter_map(|id| {
                self.contents
                    .iter()
                    .find(|pack| &folder_pack_id(&pack.name) == id)
            })
            .collect();
        self.data = PackData::build(&enabled);
        self.library = FunctionLibrary::build(enabled);
        self.data.activate();
        self.data.publish();
    }
}
