use std::{f32::consts::PI, time::Instant};

mod camera_controller;
mod mipmap_generator;

use argh::FromArgs;
use auto_instance::{AutoInstanceMaterialPlugin, AutoInstancePlugin};
use bevy::{
    core_pipeline::{
        bloom::BloomSettings,
        experimental::taa::{TemporalAntiAliasBundle, TemporalAntiAliasPlugin},
    },
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    input::mouse::MouseMotion,
    math::vec3,
    pbr::{
        CascadeShadowConfigBuilder, ScreenSpaceAmbientOcclusionBundle, TransmittedShadowReceiver,
    },
    prelude::*,
    render::view::{ColorGrading, NoFrustumCulling},
    window::{PresentMode, WindowResolution},
    winit::{UpdateMode, WinitSettings},
};
use camera_controller::CameraControllerPlugin;
use mipmap_generator::{generate_mipmaps, MipmapGeneratorPlugin, MipmapGeneratorSettings};

use crate::{
    auto_instance::{AutoInstanceMaterialRecursive, AutoInstanceMeshRecursive},
    camera_controller::CameraController,
    convert::{change_gltf_to_use_ktx2, convert_images_to_ktx2},
};

mod auto_instance;
mod convert;

#[derive(FromArgs, Resource, Clone)]
/// Config
pub struct Args {
    /// convert gltf to use ktx
    #[argh(switch)]
    convert: bool,

    /// enable auto instancing for meshes/materials
    #[argh(switch)]
    instance: bool,

    /// disable bloom, AO, AA, shadows
    #[argh(switch)]
    minimal: bool,

    /// whether to disable frustum culling.
    #[argh(switch)]
    no_frustum_culling: bool,
}

pub fn main() {
    let args: Args = argh::from_env();

    if args.convert {
        println!("This will take a few minutes");
        convert_images_to_ktx2();
        change_gltf_to_use_ktx2();
    }

    let mut app = App::new();

    app.insert_resource(args.clone())
        .insert_resource(Msaa::Off)
        .insert_resource(ClearColor(Color::rgb(0.9 * 3.0, 0.9 * 3.0, 1.0 * 3.0)))
        .insert_resource(AmbientLight {
            color: Color::rgb(0.0, 0.0, 0.0),
            brightness: 0.0,
        })
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::Continuous,
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                present_mode: PresentMode::Immediate,
                resolution: WindowResolution::new(1920.0, 1080.0).with_scale_factor_override(1.0),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(LogDiagnosticsPlugin::default())
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        // Generating mipmaps takes a minute
        .insert_resource(MipmapGeneratorSettings {
            anisotropic_filtering: 16,
            ..default()
        })
        .add_plugins((
            MipmapGeneratorPlugin,
            CameraControllerPlugin,
            TemporalAntiAliasPlugin,
        ))
        // Mipmap generation be skipped if ktx2 is used
        .add_systems(
            Update,
            (
                generate_mipmaps::<StandardMaterial>,
                proc_scene,
                input,
                benchmark,
            ),
        )
        .add_systems(Startup, setup)
        .add_systems(Update, move_directional_light);

    if args.no_frustum_culling {
        app.add_systems(Update, add_no_frustum_culling);
    }
    if args.instance {
        app.add_plugins((
            AutoInstancePlugin,
            AutoInstanceMaterialPlugin::<StandardMaterial>::default(),
        ));
    }

    app.run();
}

#[derive(Component)]
pub struct PostProcScene;

#[derive(Component)]
pub struct GrifLight;

pub fn setup(mut commands: Commands, asset_server: Res<AssetServer>, args: Res<Args>) {
    println!("Loading models, generating mipmaps");

    // Hidden Alley
    commands.spawn((
        SceneBundle {
            scene: asset_server.load("hidden_alley/ph_hidden_alley_bevy_bake.gltf#Scene0"),
            transform: Transform::from_xyz(-18.0, 0.0, 0.0),
            ..default()
        },
        PostProcScene,
        AutoInstanceMaterialRecursive,
        AutoInstanceMeshRecursive,
    ));

    // Sun
    commands.spawn((
        DirectionalLightBundle {
            transform: Transform::from_rotation(Quat::from_euler(
                EulerRot::XYZ,
                -1.8327503,
                -0.41924718,
                0.0,
            )),
            directional_light: DirectionalLight {
                color: Color::rgb_linear(0.95, 0.69268, 0.537758),
                illuminance: 3000000.0 * 0.2,
                shadows_enabled: !args.minimal,
                shadow_depth_bias: 0.04,
                shadow_normal_bias: 1.8,
            },
            cascade_shadow_config: CascadeShadowConfigBuilder {
                num_cascades: 3,
                maximum_distance: 40.0,
                ..default()
            }
            .into(),
            ..default()
        },
        GrifLight,
    ));

    let point_spot_mult = 1000.0;

    // Sky
    commands.spawn((
        PointLightBundle {
            point_light: PointLight {
                color: Color::rgb(0.8, 0.9, 0.97),
                intensity: 10000.0 * point_spot_mult,
                shadows_enabled: false,
                range: 50.0,
                radius: 3.0,
                ..default()
            },
            transform: Transform::from_xyz(-17.0, 20.0, -12.0),
            ..default()
        },
        GrifLight,
    ));

    // Sun Refl
    commands.spawn((
        SpotLightBundle {
            transform: Transform::from_xyz(-17.0, 0.1, -10.0)
                .looking_at(Vec3::new(0.0, 999.0, 0.0), Vec3::X),
            spot_light: SpotLight {
                range: 15.0,
                intensity: 5000.0 * point_spot_mult,
                color: Color::rgb(1.0, 0.97, 0.85),
                shadows_enabled: false,
                inner_angle: PI * 0.42,
                outer_angle: PI * 0.52,
                ..default()
            },
            ..default()
        },
        GrifLight,
    ));

    // Camera
    let mut cam = commands.spawn((
        Camera3dBundle {
            camera: Camera {
                hdr: true,
                ..default()
            },
            transform: Transform::from_xyz(-17.68169, 0.7696594, 4.23056)
                .looking_at(Vec3::new(-20.0, 3.5, -10.0), Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::PI / 3.0,
                ..default()
            }),
            color_grading: ColorGrading {
                #[cfg(not(feature = "bevy_main"))]
                exposure: -2.0,
                #[cfg(feature = "bevy_main")]
                global: bevy::render::view::ColorGradingGlobal {
                    exposure: -2.0,
                    ..default()
                },
                ..default()
            },
            ..default()
        },
        EnvironmentMapLight {
            diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
            specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
            intensity: 1000.0,
        },
        CameraController {
            walk_speed: 2.0,
            mouse_key_enable_mouse: MouseButton::Right,
            ..default()
        }
        .print_controls(),
    ));

    if !args.minimal {
        cam.insert((
            BloomSettings {
                intensity: 0.04,
                ..default()
            },
            FogSettings {
                color: Color::rgb(0.9 * 3.0, 0.9 * 3.0, 1.0 * 3.0),
                falloff: FogFalloff::Linear {
                    start: 4.0,
                    end: 500.0,
                },
                ..default()
            },
            TemporalAntiAliasBundle::default(),
        ))
        .insert(ScreenSpaceAmbientOcclusionBundle::default());
    }
}

pub fn all_children<F: FnMut(Entity)>(
    children: &Children,
    children_query: &Query<&Children>,
    closure: &mut F,
) {
    for child in children {
        if let Ok(children) = children_query.get(*child) {
            all_children(children, children_query, closure);
        }
        closure(*child);
    }
}

#[allow(clippy::type_complexity)]
pub fn proc_scene(
    mut commands: Commands,
    materials_query: Query<Entity, With<PostProcScene>>,
    children_query: Query<&Children>,
    has_std_mat: Query<&Handle<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lights: Query<
        Entity,
        (
            Or<(With<PointLight>, With<DirectionalLight>, With<SpotLight>)>,
            Without<GrifLight>,
        ),
    >,
    cameras: Query<Entity, With<Camera>>,
) {
    for entity in materials_query.iter() {
        if let Ok(children) = children_query.get(entity) {
            all_children(children, &children_query, &mut |entity| {
                if let Ok(mat_h) = has_std_mat.get(entity) {
                    if let Some(mat) = materials.get_mut(mat_h) {
                        match mat.alpha_mode {
                            AlphaMode::Mask(_) => {
                                mat.diffuse_transmission = 0.6;
                                mat.double_sided = true;
                                mat.cull_mode = None;
                                mat.thickness = 0.2;
                                commands.entity(entity).insert(TransmittedShadowReceiver);
                            }
                            _ => (),
                        }
                    }
                }

                // Remove Default Lights
                if lights.get(entity).is_ok() {
                    commands.entity(entity).despawn_recursive();
                }

                // Remove Default Cameras
                if cameras.get(entity).is_ok() {
                    commands.entity(entity).despawn_recursive();
                }
            });
            commands.entity(entity).remove::<PostProcScene>();
        }
    }
}
fn move_directional_light(
    mut query: Query<&mut Transform, With<DirectionalLight>>,
    mut motion_evr: EventReader<MouseMotion>,
    keys: Res<ButtonInput<KeyCode>>,
    mut e_rot: Local<Vec3>,
) {
    if !keys.pressed(KeyCode::KeyL) {
        return;
    }
    for mut trans in &mut query {
        let euler = trans.rotation.to_euler(EulerRot::XYZ);
        let euler = vec3(euler.0, euler.1, euler.2);

        for ev in motion_evr.read() {
            *e_rot = vec3(
                (euler.x.to_degrees() + ev.delta.y * 2.0).to_radians(),
                (euler.y.to_degrees() + ev.delta.x * 2.0).to_radians(),
                euler.z,
            );
        }
        let store = euler.lerp(*e_rot, 0.2);
        dbg!(store.x, store.y, store.z);
        trans.rotation = Quat::from_euler(EulerRot::XYZ, store.x, store.y, store.z);
    }
}

const CAM_POS_1: Transform = Transform {
    translation: Vec3::new(-17.68169, 0.7696594, 4.23056),
    rotation: Quat::from_array([0.09313506, 0.08030538, -0.007536669, 0.992381]),
    scale: Vec3::ONE,
};

const CAM_POS_2: Transform = Transform {
    translation: Vec3::new(-17.04247, 1.6245718, -10.109302),
    rotation: Quat::from_array([0.013972712, 0.4708807, -0.0074592647, 0.88205475]),
    scale: Vec3::ONE,
};

const CAM_POS_3: Transform = Transform {
    translation: Vec3::new(-18.569866, 1.4310247, -4.76668),
    rotation: Quat::from_array([-0.027478473, -0.8478923, -0.044159003, 0.5276112]),
    scale: Vec3::ONE,
};

fn input(input: Res<ButtonInput<KeyCode>>, mut camera: Query<&mut Transform, With<Camera>>) {
    let Ok(mut transform) = camera.get_single_mut() else {
        return;
    };
    if input.just_pressed(KeyCode::KeyI) {
        info!("{:?}", transform);
    }
    if input.just_pressed(KeyCode::Digit1) {
        *transform = CAM_POS_1
    }
    if input.just_pressed(KeyCode::Digit2) {
        *transform = CAM_POS_2
    }
    if input.just_pressed(KeyCode::Digit3) {
        *transform = CAM_POS_3
    }
}

fn benchmark(
    input: Res<ButtonInput<KeyCode>>,
    mut camera: Query<&mut Transform, With<Camera>>,
    mut bench_started: Local<Option<Instant>>,
    mut bench_frame: Local<u32>,
    mut count_per_step: Local<u32>,
    time: Res<Time>,
) {
    if input.just_pressed(KeyCode::KeyB) && bench_started.is_none() {
        *bench_started = Some(Instant::now());
        *bench_frame = 0;
        // Try to render for around 2s or at least 30 frames per step
        *count_per_step = ((2.0 / time.delta_seconds()) as u32).max(30);
        println!(
            "Starting Benchmark with {} frames per step",
            *count_per_step
        );
    }
    if bench_started.is_none() {
        return;
    }
    let Ok(mut transform) = camera.get_single_mut() else {
        return;
    };
    if *bench_frame == 0 {
        *transform = CAM_POS_1
    } else if *bench_frame == *count_per_step {
        *transform = CAM_POS_2
    } else if *bench_frame == *count_per_step * 2 {
        *transform = CAM_POS_3
    } else if *bench_frame == *count_per_step * 3 {
        let elapsed = bench_started.unwrap().elapsed().as_secs_f32();
        println!(
            "Benchmark avg cpu frame time: {:.2}ms",
            (elapsed / *bench_frame as f32) * 1000.0
        );
        *bench_started = None;
        *bench_frame = 0;
        *transform = CAM_POS_1;
    }
    *bench_frame += 1;
}

pub fn add_no_frustum_culling(
    mut commands: Commands,
    convert_query: Query<Entity, (Without<NoFrustumCulling>, With<Handle<StandardMaterial>>)>,
) {
    for entity in convert_query.iter() {
        commands.entity(entity).insert(NoFrustumCulling);
    }
}
