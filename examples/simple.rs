use std::num::NonZero;

use bevy::{
    app::{App, Startup},
    asset::{AssetServer, Assets},
    color::{Color, Srgba},
    core_pipeline::core_2d::Camera2d,
    math::{Vec2, Vec3},
    pbr::AmbientLight,
    prelude::{
        Commands, Mesh, OrthographicProjection, Plane3d, Projection, Res, ResMut, Transform,
    },
    render::mesh::Mesh2d,
    sprite::{AlphaMode2d, ColorMaterial, MeshMaterial2d},
    DefaultPlugins,
};
use bevy_rich_text3d::{Text3d, Text3dPlugin, Text3dStyling, TextAtlas};

pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(Text3dPlugin {
            load_system_fonts: true,
            ..Default::default()
        })
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 800.,
        })
        .add_systems(Startup, |mut commands: Commands, server: Res<AssetServer>, mut standard_materials: ResMut<Assets<ColorMaterial>>| {
            let mat = standard_materials.add(
                ColorMaterial {
                    texture: Some(TextAtlas::DEFAULT_IMAGE.clone_weak()),
                    alpha_mode: AlphaMode2d::Blend,
                    ..Default::default()
                }
            );
            commands.spawn((
                Text3d::new("Hello World!"),
                Text3dStyling {
                    size: 64.,
                    stroke: NonZero::new(10),
                    color: Srgba::new(0., 1., 1., 1.),
                    stroke_color: Srgba::BLACK,
                    ..Default::default()
                },
                Mesh2d::default(),
                MeshMaterial2d(mat.clone()),
            ));

            commands.spawn((
                Text3d::new("This application is powered by bevy, cosmic_text, zeno and bevy_rich_text3d!"),
                Text3dStyling {
                    color: Srgba::new(0., 1., 1., 1.),
                    ..Default::default()
                },
                Mesh2d::default(),
                MeshMaterial2d(mat.clone()),
                Transform::from_translation(Vec3::new(50., -100., 0.))
            ));

            commands.spawn((
                Mesh2d(server.add(Mesh::from(Plane3d::new(Vec3::Z, Vec2::new(200., 200.))))),
                MeshMaterial2d(mat.clone()),
                Transform::from_translation(Vec3::new(0., 100., -1.))
            ));
            commands.spawn((
                Camera2d,
                Projection::Orthographic(OrthographicProjection::default_3d()),
                Transform::from_translation(Vec3::new(0., 0., 1.))
                    .looking_at(Vec3::new(0., 0., 0.), Vec3::Y)
            ));
        })
        .run();
}
