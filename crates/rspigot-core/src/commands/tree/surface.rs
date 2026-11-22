use super::{ArgumentParser, TreeBuilder};

fn players() -> ArgumentParser {
    ArgumentParser::Entity {
        single: false,
        players_only: true,
    }
}

pub(super) fn gamemaster_surface(b: &mut TreeBuilder, root: usize) {
    let dialog = b.literal(root, "dialog", false);
    let show = b.literal(dialog, "show", false);
    let targets = b.argument(show, "targets", players(), false);
    b.argument(
        targets,
        "dialog",
        ArgumentParser::Named("minecraft:dialog"),
        true,
    );
    let clear = b.literal(dialog, "clear", false);
    b.argument(clear, "targets", players(), true);
    posteffect_surface(b, root);
    compute_surface(b, root);
    test_surface(b, root);
    fetchprofile_surface(b, root);
}

pub(super) fn posteffect_surface(b: &mut TreeBuilder, root: usize) {
    let post = b.literal(root, "posteffect", false);
    for action in ["add", "remove"] {
        let node = b.literal(post, action, false);
        let targets = b.argument(node, "targets", players(), false);
        b.argument(
            targets,
            "posteffect",
            ArgumentParser::ResourceLocation,
            true,
        );
    }
    let clear = b.literal(post, "clear", false);
    b.argument(clear, "targets", players(), true);
    let list = b.literal(post, "list", false);
    b.argument(
        list,
        "target",
        ArgumentParser::Entity {
            single: true,
            players_only: true,
        },
        true,
    );
}

pub(super) fn owner_surface(b: &mut TreeBuilder, root: usize) {
    let publish = b.literal(root, "publish", true);
    let allow = b.argument(publish, "allowCommands", ArgumentParser::Bool, true);
    b.argument(
        allow,
        "port",
        ArgumentParser::Integer {
            min: Some(0),
            max: Some(65535),
        },
        true,
    );
    b.literal(root, "unpublish", true);
    for name in ["jfr", "perf"] {
        let node = b.literal(root, name, false);
        b.literal(node, "start", true);
        b.literal(node, "stop", true);
    }
}

pub(super) fn compute_surface(b: &mut TreeBuilder, root: usize) {
    let compute = b.literal(root, "compute", false);
    let default = b.literal(compute, "default", false);
    let block = b.literal(compute, "block", false);
    let position = b.argument(block, "computePos", ArgumentParser::BlockPos, false);
    let entity = b.literal(compute, "entity", false);
    let target = b.argument(
        entity,
        "computeTarget",
        ArgumentParser::Entity {
            single: true,
            players_only: false,
        },
        false,
    );
    for parent in [default, position, target] {
        let float = b.literal(parent, "float", false);
        let provider = b.argument(
            float,
            "provider",
            ArgumentParser::Named("minecraft:context_float_provider"),
            true,
        );
        b.argument(
            provider,
            "scale",
            ArgumentParser::Float {
                min: None,
                max: None,
            },
            true,
        );
        let integer = b.literal(parent, "integer", false);
        b.argument(
            integer,
            "provider",
            ArgumentParser::Named("minecraft:context_int_provider"),
            true,
        );
    }
}

pub(super) fn test_surface(b: &mut TreeBuilder, root: usize) {
    let test = b.literal(root, "test", false);
    let selector = || ArgumentParser::ResourceSelector {
        registry: "minecraft:test_instance".to_owned(),
    };
    let int = || ArgumentParser::Integer {
        min: None,
        max: None,
    };
    let count = || ArgumentParser::Integer {
        min: Some(0),
        max: None,
    };
    for name in [
        "clearthat",
        "clearthese",
        "resetthat",
        "resetthese",
        "resetclosest",
        "stop",
    ] {
        b.literal(test, name, true);
    }
    let clearall = b.literal(test, "clearall", true);
    b.argument(clearall, "radius", int(), true);
    let create = b.literal(test, "create", false);
    let id = b.argument(create, "id", ArgumentParser::ResourceLocation, true);
    let width = b.argument(id, "width", int(), true);
    let height = b.argument(width, "height", int(), false);
    b.argument(height, "depth", int(), true);
    let locate = b.literal(test, "locate", false);
    b.argument(locate, "tests", selector(), true);
    let pos = b.literal(test, "pos", true);
    b.argument(pos, "var", ArgumentParser::Word, true);
    let run = b.literal(test, "run", false);
    let tests = b.argument(run, "tests", selector(), true);
    let times = b.argument(tests, "numberOfTimes", count(), true);
    let until = b.argument(times, "untilFailed", ArgumentParser::Bool, true);
    let rotation = b.argument(until, "rotationSteps", int(), true);
    b.argument(rotation, "testsPerRow", int(), true);
    let multiple = b.literal(test, "runmultiple", false);
    let tests = b.argument(multiple, "tests", selector(), true);
    b.argument(tests, "amount", int(), true);
    let verify = b.literal(test, "verify", false);
    b.argument(verify, "tests", selector(), true);
    for name in ["runclosest", "runthat", "runthese"] {
        let node = b.literal(test, name, true);
        let times = b.argument(node, "numberOfTimes", count(), true);
        b.argument(times, "untilFailed", ArgumentParser::Bool, true);
    }
    let failed = b.literal(test, "runfailed", true);
    let only = b.argument(failed, "onlyRequiredTests", ArgumentParser::Bool, true);
    for parent in [failed, only] {
        let times = b.argument(parent, "numberOfTimes", count(), true);
        let until = b.argument(times, "untilFailed", ArgumentParser::Bool, true);
        let rotation = b.argument(until, "rotationSteps", int(), true);
        b.argument(rotation, "testsPerRow", int(), true);
    }
}

pub(super) fn fetchprofile_surface(b: &mut TreeBuilder, root: usize) {
    let fetch = b.literal(root, "fetchprofile", false);
    let entity = b.literal(fetch, "entity", false);
    b.argument(
        entity,
        "entity",
        ArgumentParser::Entity {
            single: true,
            players_only: false,
        },
        true,
    );
    let id = b.literal(fetch, "id", false);
    b.argument(id, "id", ArgumentParser::Named("minecraft:uuid"), true);
    let name = b.literal(fetch, "name", false);
    b.argument(name, "name", ArgumentParser::GreedyString, true);
}
