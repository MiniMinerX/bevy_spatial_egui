pub mod window_mesh;

use bevy::{
    ecs::{entity::EntityHashMap, world::Command},
    prelude::*,
    render::render_resource::{Extent3d, TextureUsages},
    window::PrimaryWindow,
};
use bevy_egui::{egui, EguiContext, EguiInput, EguiRenderToTextureHandle, EguiSet};
use bevy_suis::{
    window_pointers::MouseInputMethodData, xr::HandInputMethodData,
    xr_controllers::XrControllerInputMethodData, CaptureContext, Field, InputHandler,
    InputHandlingContext, PointerInputMethod,
};
use window_mesh::construct_window_mesh;

pub struct SpatialEguiPlugin;

impl Plugin for SpatialEguiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            bevy_suis::pipe_input_ctx::<()>.pipe(update_windows),
        );
        app.add_systems(
            PreUpdate,
            forward_egui_events
                .after(EguiSet::ProcessInput)
                .before(EguiSet::BeginFrame),
        );
    }
}

fn forward_egui_events(
    mut query: Query<&mut EguiInput, With<SpatialEguiWindow>>,
    window_query: Query<&EguiInput, (With<PrimaryWindow>, Without<SpatialEguiWindow>)>,
) {
    let Ok(primary_input) = window_query.get_single() else {
        warn!("Unable to find one Primary Window!");
        return;
    };

    let events = primary_input.events.iter().filter_map(|e| match e {
        egui::Event::Copy => Some(e.clone()),
        egui::Event::Cut => Some(e.clone()),
        egui::Event::Paste(_) => Some(e.clone()),
        egui::Event::Text(_) => Some(e.clone()),
        egui::Event::Key {
            key: _,
            physical_key: _,
            pressed: _,
            repeat: _,
            modifiers: _,
        } => Some(e.clone()),
        _ => None,
    });

    for mut egui_input in query.iter_mut() {
        egui_input.events.extend(events.clone());
    }
}

fn update_windows(
    ctxs: In<Vec<InputHandlingContext>>,
    images: Res<Assets<Image>>,
    mut windows: Query<
        (
            &InputHandler,
            &SpatialEguiWindowPhysicalSize,
            &Field,
            &mut EguiInput,
            &mut EguiContext,
            &EguiRenderToTextureHandle,
        ),
        With<SpatialEguiWindow>,
    >,
    methods: Query<(
        Option<&XrControllerInputMethodData>,
        Option<&HandInputMethodData>,
        Option<&MouseInputMethodData>,
        Has<PointerInputMethod>,
    )>,
    mut state: Local<EntityHashMap<EntityHashMap<InputState>>>,
) {
    for ctx in ctxs.iter() {
        let Ok((handler, phys_size, field, mut egui_input, mut egui_ctx, texture_handle)) =
            windows.get_mut(ctx.handler)
        else {
            continue;
        };
        if handler.captured_methods.is_empty() {
            egui_input.events.push(egui::Event::PointerGone);
            continue;
        }
        let resolution = images.get(&texture_handle.0).unwrap().size_f32();
        for (method_ctx, (xr_controller_data, xr_hand_data, mouse_data, is_pointer)) in ctx
            .methods
            .iter()
            .filter_map(|ctx| methods.get(ctx.input_method).map(|v| (ctx, v)).ok())
        {
            let interaction_point = match xr_hand_data {
                // already relative
                Some(hand) => {
                    hand.get_in_relative_space(&ctx.handler_location)
                        .index
                        .tip
                        .pos
                }
                None => method_ctx.input_method_location.translation,
            };
            // already in local space
            let closest_point = field.closest_point2(&GlobalTransform::IDENTITY, interaction_point);
            let mut current_state = InputState::default();
            if closest_point.distance(interaction_point) <= f32::EPSILON && !is_pointer {
                current_state.left_button = true;
            }
            if let Some(controller) = xr_controller_data {
                current_state.left_button |= controller.trigger_pulled;
            }
            if let Some(hand) = xr_hand_data {
                let hand = hand.get_in_relative_space(&ctx.handler_location);
                current_state.left_button |= hand.index.tip.pos.distance(hand.thumb.tip.pos)
                    > (0.002 + hand.index.tip.radius + hand.thumb.tip.radius)
            }
            if let Some(mouse) = mouse_data {
                current_state.left_button |= mouse.left_button.pressed;
                current_state.right_button |= mouse.right_button.pressed;
                current_state.middle_button |= mouse.middle_button.pressed;
                current_state.discrete_scroll += mouse.discrete_scroll;
                current_state.continuous_scroll += mouse.continuous_scroll;
            }
            let uv = ((closest_point.xy() / phys_size.0.xy()) * -1.) + 0.5;
            let pos = egui::Pos2 {
                x: (uv.x * resolution.x) / egui_ctx.get_mut().pixels_per_point(),
                y: (uv.y * resolution.y) / egui_ctx.get_mut().pixels_per_point(),
            };
            egui_input.events.push(egui::Event::PointerMoved(pos));
            let last_state = state
                .entry(ctx.handler)
                .or_default()
                .entry(method_ctx.input_method)
                .or_default();
            if current_state.left_button && !last_state.left_button {
                egui_input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if !current_state.left_button && last_state.left_button {
                egui_input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if current_state.right_button && !last_state.right_button {
                egui_input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if !current_state.right_button && last_state.right_button {
                egui_input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if current_state.middle_button && !last_state.middle_button {
                egui_input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Middle,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if !current_state.middle_button && last_state.middle_button {
                egui_input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Middle,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if current_state.discrete_scroll != Vec2::ZERO {
                egui_input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::Vec2 {
                        x: current_state.discrete_scroll.x,
                        y: current_state.discrete_scroll.y,
                    },
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if current_state.continuous_scroll != Vec2::ZERO {
                egui_input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::Vec2 {
                        x: current_state.continuous_scroll.x,
                        y: current_state.continuous_scroll.y,
                    },
                    modifiers: egui::Modifiers::NONE,
                });
            }
            *last_state = current_state;
        }
    }
}

#[derive(Default)]
struct InputState {
    left_button: bool,
    middle_button: bool,
    right_button: bool,
    /// How many Lines to scroll
    discrete_scroll: Vec2,
    /// How many Pixels to scroll
    continuous_scroll: Vec2,
}

pub struct SpawnSpatialEguiWindowCommand {
    pub target_entity: Option<Entity>,
    pub position: Vec3,
    pub rotation: Quat,
    pub resolution: UVec2,
    pub unlit: bool,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Component)]
pub struct SpatialEguiWindow;

impl Command for SpawnSpatialEguiWindowCommand {
    fn apply(self, world: &mut World) {
        let mut textures = world.resource_mut::<Assets<Image>>();
        let texture = textures.add({
            let size = Extent3d {
                width: self.resolution.x,
                height: self.resolution.y,
                depth_or_array_layers: 1,
            };
            let mut output_texture = Image {
                data: vec![0; (size.width * size.height * 4) as usize],
                ..default()
            };
            output_texture.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
            output_texture.texture_descriptor.size = size;
            output_texture
        });
        let mut materials = world.remove_resource::<Assets<StandardMaterial>>().unwrap();
        let mut meshes = world.remove_resource::<Assets<Mesh>>().unwrap();
        let size = Vec3::new(
            self.height * (self.resolution.y as f32 / (self.resolution.x as f32)),
            self.height,
            0.05,
        );
        let mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(texture.clone()),
            unlit: self.unlit,
            ..Default::default()
        });
        let mesh = meshes.add(construct_window_mesh(size.xy(), size.z));
        let bundle = (
            Field::Cuboid(Cuboid::from_size(size)),
            InputHandler::new(input_surface_capture_condition),
            EguiRenderToTextureHandle(texture),
            PbrBundle {
                mesh,
                material: mat,
                transform: Transform::from_translation(self.position).with_rotation(self.rotation),
                ..Default::default()
            },
            SpatialEguiWindow,
            SpatialEguiWindowPhysicalSize(size),
        );
        world.insert_resource(materials);
        world.insert_resource(meshes);
        if let Some(target) = self.target_entity {
            world.entity_mut(target).insert(bundle);
        } else {
            world.spawn(bundle);
        }
    }
}

#[derive(Clone, Copy, Component, Debug)]
pub struct SpatialEguiWindowPhysicalSize(pub Vec3);

const MAX_CLOSE_RANGE_INTERACTION_DISTANCE: f32 = 0.15;

fn input_surface_capture_condition(
    ctx: In<CaptureContext>,
    method_query: Query<(
        Has<PointerInputMethod>,
        Option<&XrControllerInputMethodData>,
        Option<&HandInputMethodData>,
        Option<&MouseInputMethodData>,
    )>,
    // TODO: implement LastInputMethodData in bevy-suis and nuke this ugly shit!
    mut last_capture: Local<EntityHashMap<bool>>,
) -> bool {
    let Ok((is_pointer_method, xr_controller_data, xr_hand_data, mouse_data)) =
        method_query.get(ctx.input_method)
    else {
        warn!("invald input method");
        return false;
    };
    let interaction_point = match xr_hand_data {
        Some(hand) => {
            hand.get_in_relative_space(&ctx.handler_location)
                .index
                .tip
                .pos
        }
        None => ctx.input_method_location.translation,
    };
    let mut check_inputs = false;
    if is_pointer_method {
        return true;
    } else {
        let distance = interaction_point.distance(ctx.input_method_location.translation);
        if distance <= f32::EPSILON {
            return true;
        }
        if distance <= MAX_CLOSE_RANGE_INTERACTION_DISTANCE {
            check_inputs = true;
        }
    }

    let mut capture = false;
    if check_inputs {
        if let Some(mouse) = mouse_data {
            capture |= mouse.left_button.pressed;
            capture |= mouse.right_button.pressed;
            capture |= mouse.middle_button.pressed;
            capture |= mouse.discrete_scroll != Vec2::ZERO;
            capture |= mouse.continuous_scroll != Vec2::ZERO;
        }
        if let Some(hand) = xr_hand_data {
            let hand = hand.get_in_relative_space(&ctx.handler_location);
            capture |= hand.index.tip.pos.distance(hand.thumb.tip.pos)
                > (0.002 + hand.index.tip.radius + hand.thumb.tip.radius)
        }
        if let Some(controller) = xr_controller_data {
            capture |= controller.trigger_pulled;
        }
    }
    let c = capture;
    capture |= last_capture.get(&ctx.handler).copied().unwrap_or_default();
    last_capture.insert(ctx.handler, c);
    capture
}
