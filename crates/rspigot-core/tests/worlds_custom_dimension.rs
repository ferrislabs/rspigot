mod common;

use std::sync::Arc;

use common::*;
use rspigot_core::plugins::{Answer, FromPlugin, Order, ServerOrder, ToPlugin};
use rspigot_core::server::Server;
use rspigot_core::worlds::{
    GeneratorSpec, WorldKind, WorldSpec, dimension_template, register_dimension_template,
};
use rspigot_domain::Identifier;
use rspigot_domain::world::{Dimension, WorldHeight};
use rspigot_worldgen::{VoidGenerator, WorldGenerator};

fn factory(server: &mut Server<Link>, spec: &WorldSpec) -> Result<(), String> {
    let generator = WorldGenerator::Void(VoidGenerator::new(1).in_every_dimension());
    server.load_world(spec, Arc::new(generator), Arc::new(NoStore));
    Ok(())
}

fn tall_template() -> Dimension {
    let mut dimension = Dimension::overworld();
    dimension.dimension_type = Identifier::parse("demo:tall").expect("an identifier");
    dimension.height = WorldHeight::new(-32, 256).expect("a valid height");
    dimension.has_ceiling = true;
    dimension
}

fn spec(generator: GeneratorSpec) -> WorldSpec {
    WorldSpec {
        name: "tall".to_owned(),
        kind: WorldKind::Overworld,
        seed: 7,
        generator,
        structures: false,
    }
}

#[test]
fn a_registered_dimension_type_replaces_the_height_of_its_world() {
    let id = Identifier::parse("demo:tall").expect("an identifier");
    register_dimension_template(&id, tall_template());
    assert!(dimension_template(&id).is_some());
    let custom = spec(GeneratorSpec::Datapack(id)).dimension();
    assert_eq!(custom.height.min_y, -32);
    assert_eq!(custom.height.height, 256);
    assert!(custom.has_ceiling);
    assert_eq!(custom.key, spec(GeneratorSpec::Void).key());
    assert_eq!(custom.dimension_type.as_str(), "demo:tall");
    let vanilla = spec(GeneratorSpec::Void).dimension();
    assert_eq!(vanilla.height, WorldHeight::OVERWORLD);
}

#[test]
fn a_world_with_a_custom_dimension_type_loads() {
    let id = Identifier::parse("demo:tall").expect("an identifier");
    register_dimension_template(&id, tall_template());
    let server = configured(|_| {}).with_world_factory(factory);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    let request = 77_001;
    common::order(
        &mut s.server,
        1,
        FromPlugin::Action {
            request,
            order: Order::Server(ServerOrder::CreateWorld(spec(GeneratorSpec::Datapack(id)))),
        },
    );
    let answer = s
        .plugin
        .received()
        .into_iter()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer,
            } if answered == request => Some(answer),
            _ => None,
        })
        .expect("the server answers");
    assert!(!matches!(answer, Answer::Failed(_)), "{answer:?}");
}
