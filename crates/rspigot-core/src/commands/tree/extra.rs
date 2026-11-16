use super::{ArgumentParser, TreeBuilder, player_targets};

fn float(min: Option<f32>, max: Option<f32>) -> ArgumentParser {
    ArgumentParser::Float { min, max }
}

fn sound_event() -> ArgumentParser {
    ArgumentParser::Resource {
        registry: "minecraft:sound_event".to_owned(),
    }
}

const SOURCES: [&str; 11] = [
    "master", "music", "record", "weather", "block", "hostile", "neutral", "player", "ambient",
    "voice", "ui",
];

fn sound_commands(b: &mut TreeBuilder, root: usize) {
    let playsound = b.literal(root, "playsound", false);
    let sound = b.suggesting(
        playsound,
        "sound",
        ArgumentParser::ResourceLocation,
        "minecraft:available_sounds",
        false,
    );
    for source in SOURCES {
        let node = b.literal(sound, source, false);
        let targets = b.argument(node, "targets", player_targets(), true);
        let pos = b.argument(targets, "pos", ArgumentParser::Vec3, true);
        let volume = b.argument(pos, "volume", float(Some(0.0), None), true);
        let pitch = b.argument(volume, "pitch", float(Some(0.0), Some(2.0)), true);
        b.argument(pitch, "minVolume", float(Some(0.0), Some(1.0)), true);
    }
    let stopsound = b.literal(root, "stopsound", false);
    let targets = b.argument(stopsound, "targets", player_targets(), true);
    let any = b.literal(targets, "*", true);
    b.suggesting(
        any,
        "sound",
        ArgumentParser::ResourceLocation,
        "minecraft:available_sounds",
        true,
    );
    for source in SOURCES {
        let node = b.literal(targets, source, true);
        b.suggesting(
            node,
            "sound",
            ArgumentParser::ResourceLocation,
            "minecraft:available_sounds",
            true,
        );
    }
    let particle = b.literal(root, "particle", false);
    let name = b.argument(
        particle,
        "name",
        ArgumentParser::Named("minecraft:particle"),
        true,
    );
    let pos = b.argument(name, "pos", ArgumentParser::Vec3, true);
    let delta = b.argument(pos, "delta", ArgumentParser::Vec3, false);
    let speed = b.argument(delta, "speed", float(Some(0.0), None), false);
    let count = b.argument(
        speed,
        "count",
        ArgumentParser::Integer {
            min: Some(0),
            max: None,
        },
        true,
    );
    for mode in ["force", "normal"] {
        let node = b.literal(count, mode, true);
        b.argument(node, "viewers", player_targets(), true);
    }
}

pub(super) fn extra_commands(b: &mut TreeBuilder, root: usize) {
    sound_commands(b, root);
}
