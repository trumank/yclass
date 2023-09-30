use crate::{
    address::parse_address, context::InspectionContext, field::FieldKind, field::FieldResponse,
    state::StateRef, FID_M,
};
use eframe::{
    egui::{
        collapsing_header::CollapsingState, Button, CentralPanel, Context, Id, RichText,
        ScrollArea, Ui,
    },
    epaint::{vec2, Color32, FontId, Rounding},
};
use fastrand::Rng;

use super::ToolBarResponse;

macro_rules! create_change_field_type_group {
    ($ui:ident, $r:ident, $fg:ident, $bg:ident, $($size:ident),*) => {
        $(
            if $ui
                .add_sized(
                    vec2(24., $ui.available_height()),
                    Button::new(RichText::new(concat!(stringify!($size))).color(Color32::$fg)).fill(Color32::$bg),
                )
                .clicked()
            {
                *$r = Some(ToolBarResponse::ChangeKind(FieldKind::$size));
            }
            $ui.add_space(2.);
        )*
    };
}

pub struct InspectorPanel {
    address_buffer: String,
    state: StateRef,
    allow_scroll: bool,
}

impl InspectorPanel {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            allow_scroll: true,
            address_buffer: format!("0x{:X}", 0),
        }
    }

    pub fn show(&mut self, ctx: &Context) -> Option<ToolBarResponse> {
        let mut response = None;

        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                ui.visuals_mut().widgets.inactive.rounding = Rounding::ZERO;

                macro_rules! create_add_remove_group {
                            ($ui:ident, $r:ident, $var:ident, $($item:expr),*) => {
                                $(
                                    if $ui.button(stringify!($item)).clicked() {
                                        $r = Some(super::tool_bar::ToolBarResponse::$var($item));
                                        $ui.close_menu();
                                    }
                                )*
                            };
                        }

                ui.menu_button("Add", |ui| {
                    ui.set_width(64.);

                    ui.vertical_centered_justified(|ui| {
                        create_add_remove_group!(
                            ui, response, Add, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096
                        );
                    });
                })
                .response
                .on_hover_text("Adds N bytes");

                ui.menu_button("Remove", |ui| {
                    ui.set_width(64.);

                    create_add_remove_group!(ui, response, Remove, 1, 2, 4, 16, 64, 256, 1024);
                })
                .response
                .on_hover_text("Removes N fields");

                ui.menu_button("Insert", |ui| {
                    ui.set_width(64.);

                    create_add_remove_group!(ui, response, Insert, 1, 2, 4, 8, 16, 64, 256, 1024);
                })
                .response
                .on_hover_text("Inserts N bytes");

                ui.add_space(2.);
                ui.separator();
                ui.add_space(2.);

                self.field_change_ui(ui, &mut response);
            });

            ui.scope(|ui| {
                ui.style_mut().override_font_id = Some(FontId::monospace(16.));

                {
                    let state = self.state.borrow();
                    if state.process.read().is_none() {
                        ui.centered_and_justified(|ui| {
                            ui.heading("Attach to a process to begin inspection.");
                        });
                        return;
                    }

                    if state.class_list.selected_class().is_none() {
                        ui.centered_and_justified(|ui| {
                            ui.heading("Select a class from the class list to begin inspection.");
                        });
                        return;
                    }
                }

                CollapsingState::load_with_default_open(ctx, Id::new("_inspector_panel"), true)
                    .show_header(ui, |ui| {
                        let state = &mut *self.state.borrow_mut();
                        let active_class = state.class_list.selected_class()?;

                        ui.label(format!("{} - ", active_class.name));
                        ui.spacing_mut().text_edit_width = self
                            .address_buffer
                            .chars()
                            .map(|c| ui.fonts(|f| f.glyph_width(&FID_M, c)))
                            .sum::<f32>()
                            .max(160.);
                        let selected_class = state.class_list.selected_class().unwrap();

                        let r = ui.text_edit_singleline(&mut self.address_buffer);
                        if r.lost_focus() {
                            if let Some(addr) = parse_address(&self.address_buffer) {
                                selected_class.address.set(addr);
                            } else {
                                state.toasts.error("Address is in invalid format");
                            }
                        }

                        if !r.has_focus() {
                            self.address_buffer = format!("0x{:X}", selected_class.address.get());
                        }

                        Some(())
                    })
                    .body(|ui| self.inspect(ui));
            });
        });

        response
    }

    fn inspect(&mut self, ui: &mut Ui) -> Option<()> {
        let state = &mut *self.state.borrow_mut();
        let rng = Rng::with_seed(0);

        let process_lock = state.process.read();
        let mut ctx = InspectionContext {
            address: state.class_list.selected_class()?.address.get(),
            current_container: state.class_list.selected()?,
            process: process_lock.as_ref()?,
            class_list: &state.class_list,
            selection: state.selection,
            toasts: &mut state.toasts,
            current_id: Id::new(0),
            parent_id: Id::new(0),
            level_rng: &rng,
            offset: 0,
        };

        let class = state.class_list.selected_class()?;

        let mut new_class = None;
        #[allow(clippy::single_match)]
        ScrollArea::vertical()
            .auto_shrink([false, true])
            .hscroll(true)
            .enable_scrolling(self.allow_scroll)
            .show(ui, |ui| {
                match class.fields.iter().fold(None, |r, f| {
                    ctx.current_id = Id::new(rng.u64(..));
                    r.or(f.draw(ui, &mut ctx))
                }) {
                    Some(FieldResponse::NewClass(name, id)) => new_class = Some((name, id)),
                    Some(FieldResponse::LockScroll) => self.allow_scroll = false,
                    Some(FieldResponse::UnlockScroll) => self.allow_scroll = true,
                    None => {}
                }
            });
        state.selection = ctx.selection;

        if let Some((name, id)) = new_class {
            state.class_list.add_class_with_id(name, id);
        }

        Some(())
    }

    fn field_change_ui(&mut self, ui: &mut Ui, response: &mut Option<ToolBarResponse>) {
        create_change_field_type_group!(ui, response, BLACK, GOLD, Bool);

        ui.separator();
        ui.add_space(2.);

        create_change_field_type_group!(ui, response, BLACK, LIGHT_GREEN, U8, U16, U32, U64);

        ui.separator();
        ui.add_space(2.);

        create_change_field_type_group!(ui, response, BLACK, LIGHT_BLUE, I8, I16, I32, I64);

        ui.separator();
        ui.add_space(2.);

        create_change_field_type_group!(ui, response, BLACK, LIGHT_RED, F32, F64);

        ui.separator();
        ui.add_space(2.);

        create_change_field_type_group!(ui, response, BLACK, GRAY, Unk8, Unk16, Unk32, Unk64);

        ui.separator();
        ui.add_space(2.);

        create_change_field_type_group!(ui, response, BLACK, BROWN, Ptr, StrPtr, WStrPtr);
    }
}
