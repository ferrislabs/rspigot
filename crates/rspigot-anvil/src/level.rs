use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rspigot_core::config::LevelData;
use rspigot_core::ports::StoreError;
use rspigot_domain::BlockPos;
use rspigot_nbt::{Compound, Tag, read_named, write_named};

use crate::chunk::DATA_VERSION;
use crate::compression::Compression;
use crate::error::AnvilError;

pub const GAME_VERSION_NAME: &str = "26.3";
pub const SERIES: &str = "main";

const LEVEL_FILE: &str = "level.dat";
const LEVEL_BACKUP: &str = "level.dat_old";
const STORAGE_VERSION: i32 = 19133;
const OVERWORLD_CLOCK: &str = "minecraft:overworld";
const ADVANCE_TIME_RULE: &str = "minecraft:advance_time";
const SPAWN_MOBS_RULE: &str = "minecraft:spawn_mobs";
const SPAWN_MONSTERS_RULE: &str = "minecraft:spawn_monsters";
const OVERWORLD: &str = "minecraft:overworld";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn read_level_data(world_root: &Path) -> Result<Option<LevelData>, StoreError> {
    read_level(world_root).map_err(AnvilError::into_store_io)
}

pub fn write_level_data(world_root: &Path, data: &LevelData) -> Result<(), StoreError> {
    write_level(world_root, data).map_err(AnvilError::into_store_io)
}

const UID_FILE: &str = "uid.dat";

pub fn read_or_create_world_uuid(world_root: &Path) -> Result<uuid::Uuid, StoreError> {
    let path = world_root.join(UID_FILE);
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() >= 16 => {
            let mut raw = [0u8; 16];
            raw.copy_from_slice(&bytes[..16]);
            return Ok(uuid::Uuid::from_bytes(raw));
        }
        Ok(_) => tracing::warn!("{} is truncated, writing a new world uuid", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(StoreError::Io(error)),
    }
    let uuid = uuid::Uuid::new_v4();
    std::fs::create_dir_all(world_root)?;
    std::fs::write(&path, uuid.as_bytes())?;
    Ok(uuid)
}

pub(crate) fn saved_data_path(world_root: &Path, name: &str) -> PathBuf {
    world_root
        .join("data")
        .join("minecraft")
        .join(format!("{name}.dat"))
}

pub(crate) fn read_gzip_nbt(path: &Path) -> Result<Option<Compound>, AnvilError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let raw = if bytes.starts_with(&[0x1f, 0x8b]) {
        Compression::Gzip.decompress(&bytes)?
    } else {
        bytes
    };
    let mut input = raw.as_slice();
    let (_, root) = read_named(&mut input)?;
    Ok(Some(root))
}

pub(crate) fn write_gzip_nbt(
    path: &Path,
    root: &Compound,
    backup: Option<&Path>,
) -> Result<(), AnvilError> {
    let mut raw = Vec::new();
    write_named(&mut raw, "", root);
    let packed = Compression::Gzip.compress(&raw)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.{}-{unique}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("data"),
        std::process::id()
    ));
    let result = std::fs::write(&temporary, &packed).and_then(|()| {
        if let Some(backup) = backup
            && path.exists()
        {
            std::fs::rename(path, backup)?;
        }
        std::fs::rename(&temporary, path)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    Ok(result?)
}

fn check_data_version(root: &Compound, what: &str) -> Result<(), AnvilError> {
    match root.get_i64("DataVersion") {
        Some(version) if version > i64::from(DATA_VERSION) => Err(AnvilError::corrupt(format!(
            "{what} has data version {version}, newer than the supported {DATA_VERSION}"
        ))),
        _ => Ok(()),
    }
}

fn require_current_version(root: &Compound, what: &str) -> Result<(), AnvilError> {
    check_data_version(root, what)?;
    match root.get_i64("DataVersion") {
        Some(version) if version == i64::from(DATA_VERSION) => Ok(()),
        Some(version) => Err(AnvilError::corrupt(format!(
            "{what} has data version {version} and needs the vanilla upgrader before it can be rewritten"
        ))),
        None => Err(AnvilError::corrupt(format!(
            "{what} has no data version and needs the vanilla upgrader before it can be rewritten"
        ))),
    }
}

fn load_level_root(world_root: &Path) -> Result<Option<(Compound, bool)>, AnvilError> {
    let current = world_root.join(LEVEL_FILE);
    let backup = world_root.join(LEVEL_BACKUP);
    match read_gzip_nbt(&current) {
        Ok(Some(root)) => Ok(Some((root, true))),
        Ok(None) => Ok(read_gzip_nbt(&backup)?.map(|root| (root, false))),
        Err(error) => {
            tracing::warn!(path = %current.display(), %error, "failed to load world data, trying the previous copy");
            match read_gzip_nbt(&backup) {
                Ok(Some(root)) => Ok(Some((root, false))),
                _ => Err(error),
            }
        }
    }
}

fn optional_saved_data(world_root: &Path, name: &str) -> Option<Compound> {
    match read_gzip_nbt(&saved_data_path(world_root, name)) {
        Ok(Some(mut file)) => match file.remove("data") {
            Some(Tag::Compound(data)) => Some(data),
            _ => None,
        },
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(name, %error, "ignoring unreadable world data file");
            None
        }
    }
}

fn read_level(world_root: &Path) -> Result<Option<LevelData>, AnvilError> {
    let Some((root, _)) = load_level_root(world_root)? else {
        return Ok(None);
    };
    let data = root
        .get_compound("Data")
        .ok_or_else(|| AnvilError::corrupt("level.dat has no Data compound"))?;
    check_data_version(data, "level.dat")?;
    let mut level = LevelData::default();
    if let Some(name) = data.get_str("LevelName") {
        name.clone_into(&mut level.name);
    }
    let (spawn, angle) = read_spawn(data);
    level.spawn = if data.get_bool("initialized").unwrap_or(true) {
        spawn
    } else {
        None
    };
    level.spawn_angle = angle.unwrap_or(level.spawn_angle);
    level.game_time = data.get_i64("Time").unwrap_or(level.game_time);
    level.day_time = data.get_i64("DayTime").unwrap_or(level.day_time);
    level.raining = data.get_bool("raining").unwrap_or(level.raining);
    level.thundering = data.get_bool("thundering").unwrap_or(level.thundering);
    if let Some(rule) = data
        .get_compound("GameRules")
        .and_then(|rules| rules.get("doDaylightCycle"))
    {
        level.daylight_cycle = flag(rule).unwrap_or(level.daylight_cycle);
    }
    if let Some(seed) = data
        .get_compound("WorldGenSettings")
        .and_then(|settings| settings.get_i64("seed"))
    {
        level.seed = seed;
    }
    if let Some(settings) = optional_saved_data(world_root, "world_gen_settings")
        && let Some(seed) = settings.get_i64("seed")
    {
        level.seed = seed;
    }
    if let Some(rules) = optional_saved_data(world_root, "game_rules") {
        let rule = |key: &str, current: bool| rules.get(key).and_then(flag).unwrap_or(current);
        level.daylight_cycle = rule(ADVANCE_TIME_RULE, level.daylight_cycle);
        level.spawn_mobs = rule(SPAWN_MOBS_RULE, level.spawn_mobs);
        level.spawn_monsters = rule(SPAWN_MONSTERS_RULE, level.spawn_monsters);
    }
    if let Some(weather) = optional_saved_data(world_root, "weather") {
        level.raining = weather.get_bool("raining").unwrap_or(level.raining);
        level.thundering = weather.get_bool("thundering").unwrap_or(level.thundering);
    }
    if let Some(clocks) = optional_saved_data(world_root, "world_clocks")
        && let Some(ticks) = clocks
            .get_compound(OVERWORLD_CLOCK)
            .and_then(|clock| clock.get_i64("total_ticks"))
    {
        level.day_time = ticks;
    }
    level.raids = optional_saved_data(world_root, "raids");
    Ok(Some(level))
}

fn flag(tag: &Tag) -> Option<bool> {
    match tag {
        Tag::String(text) => match text.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        other => other.as_i64().map(|value| value != 0),
    }
}

fn read_spawn(data: &Compound) -> (Option<BlockPos>, Option<f32>) {
    if let Some(spawn) = data.get_compound("spawn") {
        let pos = match spawn.get_int_array("pos") {
            Some([x, y, z]) => Some(BlockPos::new(*x, *y, *z)),
            _ => None,
        };
        return (pos, spawn.get_f64("yaw").map(|yaw| yaw as f32));
    }
    let legacy = match (
        data.get_i32("SpawnX"),
        data.get_i32("SpawnY"),
        data.get_i32("SpawnZ"),
    ) {
        (Some(x), Some(y), Some(z)) => Some(BlockPos::new(x, y, z)),
        _ => None,
    };
    (legacy, data.get_f64("SpawnAngle").map(|angle| angle as f32))
}

fn write_level(world_root: &Path, level: &LevelData) -> Result<(), AnvilError> {
    let loaded = load_level_root(world_root)?;
    let fresh = loaded.is_none();
    let rotate_backup = loaded
        .as_ref()
        .is_some_and(|(_, from_current)| *from_current);
    let existing = loaded.map(|(root, _)| root);
    if let Some(root) = &existing {
        let data = root
            .get_compound("Data")
            .ok_or_else(|| AnvilError::corrupt("level.dat has no Data compound"))?;
        require_current_version(data, "level.dat")?;
    }
    update_saved_data(world_root, "world_gen_settings", |data| {
        let mut changed = data.get("seed") != Some(&Tag::Long(level.seed));
        data.insert("seed", level.seed);
        for (key, value) in [("generate_structures", true), ("bonus_chest", false)] {
            if !data.contains_key(key) {
                data.insert(key, value);
                changed = true;
            }
        }
        if !data.contains_key("dimensions") {
            data.insert("dimensions", Compound::new());
            changed = true;
        }
        changed
    })?;
    update_saved_data(world_root, "game_rules", |data| {
        let mut changed = false;
        for (key, value) in [
            (ADVANCE_TIME_RULE, level.daylight_cycle),
            (SPAWN_MOBS_RULE, level.spawn_mobs),
            (SPAWN_MONSTERS_RULE, level.spawn_monsters),
        ] {
            changed |= data.get_bool(key) != Some(value);
            data.insert(key, value);
        }
        changed
    })?;
    update_saved_data(world_root, "weather", |data| {
        let mut changed = data.get_bool("raining") != Some(level.raining)
            || data.get_bool("thundering") != Some(level.thundering);
        data.insert("raining", level.raining);
        data.insert("thundering", level.thundering);
        for key in ["clear_weather_time", "rain_time", "thunder_time"] {
            if !data.contains_key(key) {
                data.insert(key, 0);
                changed = true;
            }
        }
        changed
    })?;
    update_saved_data(world_root, "world_clocks", |data| {
        let mut clock = match data.remove(OVERWORLD_CLOCK) {
            Some(Tag::Compound(clock)) => clock,
            _ => Compound::new()
                .with("partial_tick", 0.0f32)
                .with("rate", 1.0f32)
                .with("paused", false),
        };
        let changed = clock.get("total_ticks") != Some(&Tag::Long(level.day_time));
        clock.insert("total_ticks", level.day_time);
        data.insert(OVERWORLD_CLOCK, clock);
        changed
    })?;
    if let Some(raids) = &level.raids {
        update_saved_data(world_root, "raids", |data| {
            let changed = data != raids;
            data.clone_from(raids);
            changed
        })?;
    }
    let mut root = existing.unwrap_or_default();
    let mut data = match root.remove("Data") {
        Some(Tag::Compound(data)) => data,
        _ => Compound::new(),
    };
    apply_level_fields(&mut data, level, fresh);
    root.insert("Data", data);
    let backup = world_root.join(LEVEL_BACKUP);
    write_gzip_nbt(
        &world_root.join(LEVEL_FILE),
        &root,
        rotate_backup.then_some(backup.as_path()),
    )
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn apply_level_fields(data: &mut Compound, level: &LevelData, fresh: bool) {
    if fresh {
        data.insert(
            "ServerBrands",
            Tag::List(vec![Tag::String("rspigot".to_owned())]),
        );
        data.insert("WasModded", false);
        data.insert("GameType", 0);
        data.insert("allowCommands", false);
        data.insert(
            "difficulty_settings",
            Compound::new()
                .with("difficulty", "easy")
                .with("hardcore", false)
                .with("locked", false),
        );
        data.insert(
            "DataPacks",
            Compound::new()
                .with(
                    "Enabled",
                    Tag::List(vec![Tag::String("vanilla".to_owned())]),
                )
                .with("Disabled", Tag::List(Vec::new())),
        );
        data.insert(
            "enabled_features",
            Tag::List(vec![Tag::String("minecraft:vanilla".to_owned())]),
        );
    }
    data.insert(
        "Version",
        Compound::new()
            .with("Name", GAME_VERSION_NAME)
            .with("Id", DATA_VERSION)
            .with("Snapshot", false)
            .with("Series", SERIES),
    );
    let mut history = match data.remove("version_history") {
        Some(Tag::List(history)) => history,
        _ => Vec::new(),
    };
    if history.last().and_then(Tag::as_i64) != Some(i64::from(DATA_VERSION)) {
        history.push(Tag::Int(DATA_VERSION));
    }
    data.insert("version_history", Tag::List(history));
    data.insert("DataVersion", DATA_VERSION);
    let mut spawn = match data.remove("spawn") {
        Some(Tag::Compound(spawn)) => spawn,
        _ => Compound::new(),
    };
    if !spawn.contains_key("dimension") {
        spawn.insert("dimension", OVERWORLD);
    }
    if !spawn.contains_key("pitch") {
        spawn.insert("pitch", 0.0f32);
    }
    match level.spawn {
        Some(pos) => {
            spawn.insert("pos", Tag::IntArray(vec![pos.x, pos.y, pos.z]));
        }
        None if !spawn.contains_key("pos") => {
            spawn.insert("pos", Tag::IntArray(vec![0, 0, 0]));
        }
        None => {}
    }
    spawn.insert("yaw", level.spawn_angle);
    data.insert("spawn", spawn);
    data.insert("Time", level.game_time);
    data.insert("LastPlayed", now_millis());
    data.insert("LevelName", level.name.as_str());
    data.insert("version", STORAGE_VERSION);
    data.insert("initialized", level.spawn.is_some());
}

pub(crate) fn update_saved_data(
    world_root: &Path,
    name: &str,
    update: impl FnOnce(&mut Compound) -> bool,
) -> Result<bool, AnvilError> {
    let path = saved_data_path(world_root, name);
    let existing = match read_gzip_nbt(&path) {
        Ok(existing) => existing,
        Err(AnvilError::Io(error)) => return Err(error.into()),
        Err(error) => {
            tracing::warn!(name, %error, "replacing an unreadable world data file");
            None
        }
    };
    let existed = existing.is_some();
    let mut root = existing.unwrap_or_default();
    if root.contains_key("DataVersion") {
        require_current_version(&root, name)?;
    }
    let mut data = match root.remove("data") {
        Some(Tag::Compound(data)) => data,
        _ => Compound::new(),
    };
    let changed = update(&mut data);
    if existed && !changed {
        return Ok(false);
    }
    root.insert("data", data);
    if !root.contains_key("DataVersion") {
        root.insert("DataVersion", DATA_VERSION);
    }
    write_gzip_nbt(&path, &root, None)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use rspigot_domain::BlockPos;
    use rspigot_nbt::Tag;

    fn sample() -> LevelData {
        LevelData {
            name: "Test World".to_owned(),
            seed: -1_234_567_890_123,
            spawn: Some(BlockPos::new(10, 70, -20)),
            spawn_angle: 90.5,
            game_time: 123_456,
            day_time: 6000,
            daylight_cycle: false,
            raining: true,
            thundering: true,
            spawn_mobs: true,
            spawn_monsters: false,
            uuid: None,
            raids: None,
        }
    }

    fn read_file(root: &Path, relative: &str) -> Compound {
        read_gzip_nbt(&root.join(relative)).unwrap().unwrap()
    }

    fn write_file(root: &Path, relative: &str, compound: &Compound) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut raw = Vec::new();
        rspigot_nbt::write_named(&mut raw, "", compound);
        let packed = crate::Compression::Gzip.compress(&raw).unwrap();
        std::fs::write(path, packed).unwrap();
    }

    #[test]
    fn missing_world_reads_as_none() {
        let dir = TempDir::new("level-none");
        assert!(read_level_data(dir.path()).unwrap().is_none());
        assert!(
            read_level_data(&dir.path().join("absent"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn fresh_world_round_trips() {
        let dir = TempDir::new("level-fresh");
        let root = dir.path().join("world");
        write_level_data(&root, &sample()).unwrap();
        assert_eq!(read_level_data(&root).unwrap(), Some(sample()));
        let again = LevelData {
            game_time: 999,
            raining: false,
            ..sample()
        };
        write_level_data(&root, &again).unwrap();
        assert_eq!(read_level_data(&root).unwrap(), Some(again));
    }

    #[test]
    fn files_follow_the_26_3_layout() {
        let dir = TempDir::new("level-layout");
        let root = dir.path();
        write_level_data(root, &sample()).unwrap();

        let level = read_file(root, "level.dat");
        let data = level.get_compound("Data").unwrap();
        assert_eq!(data.get_str("LevelName"), Some("Test World"));
        assert_eq!(data.get_i32("DataVersion"), Some(5023));
        assert_eq!(data.get_i64("Time"), Some(123_456));
        assert_eq!(data.get_bool("initialized"), Some(true));
        assert_eq!(data.get_i32("version"), Some(19133));
        assert!(data.get_i64("LastPlayed").unwrap() > 1_600_000_000_000);
        let version = data.get_compound("Version").unwrap();
        assert_eq!(version.get_str("Name"), Some("26.3"));
        assert_eq!(version.get_i32("Id"), Some(5023));
        assert_eq!(version.get_bool("Snapshot"), Some(false));
        assert_eq!(version.get_str("Series"), Some("main"));
        let spawn = data.get_compound("spawn").unwrap();
        assert_eq!(spawn.get_str("dimension"), Some("minecraft:overworld"));
        assert_eq!(spawn.get_int_array("pos"), Some(&[10, 70, -20][..]));
        assert_eq!(spawn.get("yaw"), Some(&Tag::Float(90.5)));
        assert_eq!(
            data.get_list("version_history"),
            Some(&[Tag::Int(5023)][..])
        );
        assert!(data.get("SpawnX").is_none());
        assert!(data.get("DayTime").is_none());
        assert!(data.get("WorldGenSettings").is_none());

        let settings = read_file(root, "data/minecraft/world_gen_settings.dat");
        assert_eq!(settings.get_i32("DataVersion"), Some(5023));
        let settings = settings.get_compound("data").unwrap();
        assert_eq!(settings.get("seed"), Some(&Tag::Long(-1_234_567_890_123)));
        assert!(settings.get_compound("dimensions").is_some());
        assert!(settings.contains_key("generate_structures"));
        assert!(settings.contains_key("bonus_chest"));

        let weather = read_file(root, "data/minecraft/weather.dat");
        let weather = weather.get_compound("data").unwrap();
        assert_eq!(weather.get_bool("raining"), Some(true));
        assert_eq!(weather.get_bool("thundering"), Some(true));
        assert_eq!(weather.get_i32("rain_time"), Some(0));
        assert_eq!(weather.get_i32("thunder_time"), Some(0));
        assert_eq!(weather.get_i32("clear_weather_time"), Some(0));

        let clocks = read_file(root, "data/minecraft/world_clocks.dat");
        let overworld = clocks
            .get_compound("data")
            .unwrap()
            .get_compound("minecraft:overworld")
            .unwrap();
        assert_eq!(overworld.get("total_ticks"), Some(&Tag::Long(6000)));

        let rules = read_file(root, "data/minecraft/game_rules.dat");
        let rules = rules.get_compound("data").unwrap();
        assert_eq!(rules.get_bool("minecraft:advance_time"), Some(false));
        assert_eq!(rules.get_bool("minecraft:spawn_mobs"), Some(true));
        assert_eq!(rules.get_bool("minecraft:spawn_monsters"), Some(false));
    }

    #[test]
    fn unset_spawn_marks_the_world_uninitialized() {
        let dir = TempDir::new("level-spawn");
        let level = LevelData {
            spawn: None,
            ..sample()
        };
        write_level_data(dir.path(), &level).unwrap();
        let file = read_file(dir.path(), "level.dat");
        let data = file.get_compound("Data").unwrap();
        assert_eq!(data.get_bool("initialized"), Some(false));
        assert!(data.get_compound("spawn").is_some());
        assert_eq!(read_level_data(dir.path()).unwrap(), Some(level));
        write_level_data(dir.path(), &sample()).unwrap();
        assert_eq!(read_level_data(dir.path()).unwrap(), Some(sample()));
    }

    fn vanilla_level() -> Compound {
        Compound::new().with(
            "Data",
            Compound::new()
                .with(
                    "ServerBrands",
                    Tag::List(vec![Tag::String("spigot".into())]),
                )
                .with("WasModded", false)
                .with(
                    "Version",
                    Compound::new()
                        .with("Name", "26.2")
                        .with("Id", 5000)
                        .with("Snapshot", true)
                        .with("Series", "snapshot"),
                )
                .with(
                    "version_history",
                    Tag::List(vec![Tag::Int(4900), Tag::Int(5000)]),
                )
                .with("DataVersion", 5023)
                .with("GameType", 1)
                .with(
                    "spawn",
                    Compound::new()
                        .with("dimension", "minecraft:the_nether")
                        .with("pos", Tag::IntArray(vec![1, 2, 3]))
                        .with("yaw", 12.0f32)
                        .with("pitch", 3.0f32),
                )
                .with("Time", 5i64)
                .with("LastPlayed", 1i64)
                .with("LevelName", "Old Name")
                .with("version", 19133)
                .with("allowCommands", true)
                .with("initialized", true)
                .with(
                    "difficulty_settings",
                    Compound::new()
                        .with("difficulty", "hard")
                        .with("hardcore", false)
                        .with("locked", true),
                )
                .with("Bukkit.Version", "26.3-R0.1-SNAPSHOT")
                .with("BukkitValues", Compound::new().with("plugin:k", 1))
                .with("FutureKey", Tag::LongArray(vec![9, 8])),
        )
    }

    #[test]
    fn rewriting_preserves_unknown_keys_and_files() {
        let dir = TempDir::new("level-preserve");
        let root = dir.path();
        write_file(root, "level.dat", &vanilla_level());
        write_file(
            root,
            "data/minecraft/world_gen_settings.dat",
            &Compound::new().with("DataVersion", 5023).with(
                "data",
                Compound::new()
                    .with("seed", 5i64)
                    .with("generate_structures", false)
                    .with("bonus_chest", true)
                    .with(
                        "dimensions",
                        Compound::new().with(
                            "minecraft:overworld",
                            Compound::new().with("type", "minecraft:overworld"),
                        ),
                    ),
            ),
        );
        write_file(
            root,
            "data/minecraft/game_rules.dat",
            &Compound::new().with("DataVersion", 5023).with(
                "data",
                Compound::new()
                    .with("minecraft:keep_inventory", true)
                    .with("minecraft:advance_time", true)
                    .with("minecraft:random_tick_speed", 7),
            ),
        );
        write_file(
            root,
            "data/minecraft/weather.dat",
            &Compound::new().with("DataVersion", 5023).with(
                "data",
                Compound::new()
                    .with("clear_weather_time", 11)
                    .with("rain_time", 22)
                    .with("thunder_time", 33)
                    .with("raining", false)
                    .with("thundering", false),
            ),
        );
        write_file(
            root,
            "data/minecraft/world_clocks.dat",
            &Compound::new().with("DataVersion", 5023).with(
                "data",
                Compound::new()
                    .with(
                        "minecraft:overworld",
                        Compound::new()
                            .with("total_ticks", 77i64)
                            .with("partial_tick", 0.5f32)
                            .with("rate", 2.0f32)
                            .with("paused", true),
                    )
                    .with(
                        "minecraft:the_end",
                        Compound::new().with("total_ticks", 3i64),
                    ),
            ),
        );

        let before = read_level_data(root).unwrap().unwrap();
        assert_eq!(before.name, "Old Name");
        assert_eq!(before.seed, 5);
        assert_eq!(before.spawn, Some(BlockPos::new(1, 2, 3)));
        assert_eq!(before.spawn_angle, 12.0);
        assert_eq!(before.game_time, 5);
        assert_eq!(before.day_time, 77);
        assert!(before.daylight_cycle);
        assert!(!before.raining);

        write_level_data(root, &sample()).unwrap();

        let level = read_file(root, "level.dat");
        let data = level.get_compound("Data").unwrap();
        assert_eq!(data.get_str("LevelName"), Some("Test World"));
        assert_eq!(data.get_i64("Time"), Some(123_456));
        assert_eq!(data.get_i32("GameType"), Some(1));
        assert_eq!(data.get_bool("allowCommands"), Some(true));
        assert_eq!(data.get_str("Bukkit.Version"), Some("26.3-R0.1-SNAPSHOT"));
        assert_eq!(
            data.get_compound("BukkitValues"),
            Some(&Compound::new().with("plugin:k", 1))
        );
        assert_eq!(data.get("FutureKey"), Some(&Tag::LongArray(vec![9, 8])));
        assert_eq!(
            data.get_compound("difficulty_settings")
                .unwrap()
                .get_str("difficulty"),
            Some("hard")
        );
        assert_eq!(
            data.get_list("ServerBrands"),
            Some(&[Tag::String("spigot".into())][..])
        );
        let spawn = data.get_compound("spawn").unwrap();
        assert_eq!(spawn.get_str("dimension"), Some("minecraft:the_nether"));
        assert_eq!(spawn.get_int_array("pos"), Some(&[10, 70, -20][..]));
        assert_eq!(spawn.get("pitch"), Some(&Tag::Float(3.0)));
        let version = data.get_compound("Version").unwrap();
        assert_eq!(version.get_str("Name"), Some("26.3"));
        assert_eq!(version.get_i32("Id"), Some(5023));
        assert_eq!(version.get_bool("Snapshot"), Some(false));
        assert_eq!(
            data.get_list("version_history"),
            Some(&[Tag::Int(4900), Tag::Int(5000), Tag::Int(5023)][..])
        );

        let settings = read_file(root, "data/minecraft/world_gen_settings.dat");
        let settings = settings.get_compound("data").unwrap();
        assert_eq!(settings.get("seed"), Some(&Tag::Long(-1_234_567_890_123)));
        assert_eq!(settings.get_bool("generate_structures"), Some(false));
        assert_eq!(settings.get_bool("bonus_chest"), Some(true));
        assert!(
            settings
                .get_compound("dimensions")
                .unwrap()
                .contains_key("minecraft:overworld")
        );

        let rules = read_file(root, "data/minecraft/game_rules.dat");
        let rules = rules.get_compound("data").unwrap();
        assert_eq!(rules.get_bool("minecraft:advance_time"), Some(false));
        assert_eq!(rules.get_bool("minecraft:keep_inventory"), Some(true));
        assert_eq!(rules.get_i32("minecraft:random_tick_speed"), Some(7));

        let weather = read_file(root, "data/minecraft/weather.dat");
        let weather = weather.get_compound("data").unwrap();
        assert_eq!(weather.get_i32("clear_weather_time"), Some(11));
        assert_eq!(weather.get_i32("rain_time"), Some(22));
        assert_eq!(weather.get_i32("thunder_time"), Some(33));
        assert_eq!(weather.get_bool("raining"), Some(true));
        assert_eq!(weather.get_bool("thundering"), Some(true));

        let clocks = read_file(root, "data/minecraft/world_clocks.dat");
        let clocks = clocks.get_compound("data").unwrap();
        let overworld = clocks.get_compound("minecraft:overworld").unwrap();
        assert_eq!(overworld.get("total_ticks"), Some(&Tag::Long(6000)));
        assert_eq!(overworld.get("partial_tick"), Some(&Tag::Float(0.5)));
        assert_eq!(overworld.get("rate"), Some(&Tag::Float(2.0)));
        assert_eq!(overworld.get_bool("paused"), Some(true));
        assert_eq!(
            clocks
                .get_compound("minecraft:the_end")
                .unwrap()
                .get("total_ticks"),
            Some(&Tag::Long(3))
        );
    }

    #[test]
    fn saved_data_is_only_rewritten_when_it_changes() {
        let dir = TempDir::new("level-skip");
        let changed = update_saved_data(dir.path(), "weather", |data| {
            data.insert("raining", true);
            true
        })
        .unwrap();
        assert!(changed);
        let changed = update_saved_data(dir.path(), "weather", |_| false).unwrap();
        assert!(!changed);
        assert_eq!(
            read_file(dir.path(), "data/minecraft/weather.dat")
                .get_compound("data")
                .unwrap()
                .get_bool("raining"),
            Some(true)
        );
        write_level_data(dir.path(), &sample()).unwrap();
        let before = std::fs::read(saved_data_path(dir.path(), "weather")).unwrap();
        write_level_data(dir.path(), &sample()).unwrap();
        assert_eq!(
            std::fs::read(saved_data_path(dir.path(), "weather")).unwrap(),
            before
        );
    }

    #[test]
    fn pre_26_1_level_dat_reads_best_effort() {
        let dir = TempDir::new("level-legacy");
        write_file(
            dir.path(),
            "level.dat",
            &Compound::new().with(
                "Data",
                Compound::new()
                    .with("DataVersion", 3953)
                    .with("LevelName", "Legacy")
                    .with("SpawnX", 100)
                    .with("SpawnY", 64)
                    .with("SpawnZ", -100)
                    .with("SpawnAngle", 45.0f32)
                    .with("Time", 1000i64)
                    .with("DayTime", 18_000i64)
                    .with("raining", true)
                    .with("thundering", false)
                    .with(
                        "GameRules",
                        Compound::new().with("doDaylightCycle", "false"),
                    )
                    .with("WorldGenSettings", Compound::new().with("seed", 4242i64)),
            ),
        );
        let level = read_level_data(dir.path()).unwrap().unwrap();
        assert_eq!(level.name, "Legacy");
        assert_eq!(level.seed, 4242);
        assert_eq!(level.spawn, Some(BlockPos::new(100, 64, -100)));
        assert_eq!(level.spawn_angle, 45.0);
        assert_eq!(level.game_time, 1000);
        assert_eq!(level.day_time, 18_000);
        assert!(!level.daylight_cycle);
        assert!(level.raining);
        assert!(!level.thundering);
        assert!(write_level_data(dir.path(), &sample()).is_err());
    }

    #[test]
    fn newer_world_versions_are_refused() {
        let dir = TempDir::new("level-newer");
        let mut level = vanilla_level();
        if let Some(Tag::Compound(data)) = level.get_mut("Data") {
            data.insert("DataVersion", 5024);
        }
        write_file(dir.path(), "level.dat", &level);
        assert!(read_level_data(dir.path()).is_err());
        assert!(write_level_data(dir.path(), &sample()).is_err());
    }

    #[test]
    fn the_previous_level_dat_is_kept_and_used_as_fallback() {
        let dir = TempDir::new("level-backup");
        let first = LevelData {
            game_time: 1,
            ..sample()
        };
        let second = LevelData {
            game_time: 2,
            ..sample()
        };
        write_level_data(dir.path(), &first).unwrap();
        assert!(!dir.path().join("level.dat_old").exists());
        write_level_data(dir.path(), &second).unwrap();
        assert!(dir.path().join("level.dat_old").is_file());
        assert_eq!(
            read_file(dir.path(), "level.dat_old")
                .get_compound("Data")
                .unwrap()
                .get_i64("Time"),
            Some(1)
        );
        std::fs::write(dir.path().join("level.dat"), b"garbage").unwrap();
        assert_eq!(read_level_data(dir.path()).unwrap().unwrap().game_time, 1);
        let third = LevelData {
            game_time: 3,
            ..sample()
        };
        write_level_data(dir.path(), &third).unwrap();
        assert_eq!(
            read_file(dir.path(), "level.dat_old")
                .get_compound("Data")
                .unwrap()
                .get_i64("Time"),
            Some(1)
        );
        assert_eq!(read_level_data(dir.path()).unwrap().unwrap().game_time, 3);
        std::fs::write(dir.path().join("level.dat"), b"garbage").unwrap();
        std::fs::remove_file(dir.path().join("level.dat_old")).unwrap();
        assert!(read_level_data(dir.path()).is_err());
        std::fs::remove_file(dir.path().join("level.dat")).unwrap();
        assert!(read_level_data(dir.path()).unwrap().is_none());
    }

    #[test]
    fn no_temporary_files_are_left_behind() {
        let dir = TempDir::new("level-tmp");
        write_level_data(dir.path(), &sample()).unwrap();
        write_level_data(dir.path(), &sample()).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .chain(std::fs::read_dir(dir.path().join("data/minecraft")).unwrap())
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert!(names.iter().all(|n| !n.contains("tmp")), "{names:?}");
    }
}
