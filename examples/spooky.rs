use bevy::{
    app::{App, Startup},
    asset::{Asset, Assets},
    color::Color,
    math::Vec3,
    pbr::{
        AmbientLight, ExtendedMaterial, MaterialExtension, MaterialPlugin, MeshMaterial3d,
        StandardMaterial,
    },
    prelude::{
        AlphaMode, Camera3d, Commands, Mesh3d, OrthographicProjection, Projection, ResMut,
        Transform,
    },
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderRef},
    DefaultPlugins,
};
use bevy_rich_text3d::{
    GlyphMeta, LoadSystemFontPlugin, Text3d, Text3dPlugin, Text3dPluginSettings, Text3dStyling,
    DEFAULT_GLYPH_ATLAS,
};

#[derive(Debug, Clone, TypePath, AsBindGroup, Asset)]
pub struct SpookyShader {
    #[uniform(100)]
    pub frequency: f32,
    #[uniform(101)]
    pub intensity: f32,
}

impl MaterialExtension for SpookyShader {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path("wiggle.wgsl".into())
    }
}

pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(MaterialPlugin::<ExtendedMaterial<StandardMaterial, SpookyShader>>::default())
        .insert_resource(Text3dPluginSettings {
            default_atlas_dimension: (1024, 512),
            scale_factor: 2.,
        })
        .add_plugins(Text3dPlugin)
        .add_plugins(LoadSystemFontPlugin)
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 800.,
        })
        .add_systems(Startup, |mut commands: Commands,
            mut mats: ResMut<Assets<ExtendedMaterial<StandardMaterial, SpookyShader>>>,
        | {
            let mat = mats.add(
                ExtendedMaterial {
                    base: StandardMaterial {
                        base_color_texture: Some(DEFAULT_GLYPH_ATLAS.clone_weak()),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        ..Default::default()
                    },
                    extension: SpookyShader {
                        frequency: 1.,
                        intensity: 14.,
                    },
                }
            );
            commands.spawn((
                Text3d::parse_raw("Something {s-4, s-white, transparent, v-1:SPOOKY} is happening!").unwrap(),
                Text3dStyling {
                    size: 64.0,
                    uv1: (GlyphMeta::PerGlyphAdvance, GlyphMeta::MagicNumber),
                    ..Default::default()
                },
                Mesh3d::default(),
                MeshMaterial3d(mat.clone()),
            ));
            commands.spawn((
                Camera3d::default(),
                Projection::Orthographic(OrthographicProjection::default_3d()),
                Transform::from_translation(Vec3::new(0., 0., 1.))
                    .looking_at(Vec3::new(0., 0., 0.), Vec3::Y)
            ));
        })
        .run();
}
