use crate::{
    context::Selection,
    field::allocate_padding,
    gui::{ClassListPanel, InspectorPanel, ToolBarPanel, ToolBarResponse},
    process::Process,
    state::StateRef,
};
use eframe::{egui::Context, epaint::Color32, App, Frame};
use std::{sync::Once, time::Duration};

pub struct YClassApp {
    class_list: ClassListPanel,
    inspector: InspectorPanel,
    tool_bar: ToolBarPanel,
    state: StateRef,
}

impl YClassApp {
    pub fn new(state: StateRef) -> Self {
        Self {
            class_list: ClassListPanel::new(state),
            inspector: InspectorPanel::new(state),
            tool_bar: ToolBarPanel::new(state),
            state,
        }
    }

    fn handle_reponse(&mut self, frame: &mut Frame, response: Option<ToolBarResponse>) {
        match response {
            Some(ToolBarResponse::Add(n)) => {
                let state = &mut *self.state.borrow_mut();

                if let Some(cid) = state
                    .selection
                    .map(|s| s.container_id)
                    .or_else(|| state.class_list.selected())
                {
                    let class = state.class_list.by_id_mut(cid).unwrap();
                    class.fields.extend(allocate_padding(n));

                    state.dummy = false;
                }
            }
            Some(ToolBarResponse::Remove(n)) => {
                let state = &mut *self.state.borrow_mut();

                if let Some(Selection {
                    container_id,
                    field_id,
                    ..
                }) = state.selection
                {
                    let class = state.class_list.by_id_mut(container_id).unwrap();
                    let mut discrd_sel = false;
                    let pos = class
                        .fields
                        .iter()
                        .position(|f| {
                            discrd_sel |= state
                                .selection
                                .map(|s| s.field_id == f.id())
                                .unwrap_or(false);

                            f.id() == field_id
                        })
                        .unwrap();
                    if discrd_sel {
                        state.selection = None;
                    }

                    let from = pos.min(class.fields.len());
                    let to = (pos + n).min(class.fields.len());

                    class.fields.drain(from..to);
                    state.dummy = false;
                }
            }
            Some(ToolBarResponse::Insert(n)) => {
                let state = &mut *self.state.borrow_mut();

                if let Some(Selection {
                    container_id,
                    field_id,
                    ..
                }) = state.selection
                {
                    let class = state.class_list.by_id_mut(container_id).unwrap();
                    let pos = class
                        .fields
                        .iter()
                        .position(|f| f.id() == field_id)
                        .unwrap();
                    let mut padding = allocate_padding(n);

                    while let Some(field) = padding.pop() {
                        class.fields.insert(pos, field);
                    }

                    state.dummy = false;
                }
            }
            Some(ToolBarResponse::ChangeKind(new)) => {
                let state = &mut *self.state.borrow_mut();

                if let Some(Selection {
                    container_id,
                    field_id,
                    ..
                }) = state.selection
                {
                    let class = state.class_list.by_id_mut(container_id).unwrap();
                    let pos = class
                        .fields
                        .iter()
                        .position(|f| f.id() == field_id)
                        .unwrap();

                    let (old_size, old_name) = (class.fields[pos].size(), class.fields[pos].name());
                    if old_size > new.size() {
                        let mut padding = allocate_padding(old_size - new.size());
                        class.fields[pos] = new.into_field(old_name);
                        while let Some(pad) = padding.pop() {
                            class.fields.insert(pos + 1, pad);
                        }

                        state.selection.as_mut().unwrap().field_id = class.fields[pos].id();
                    } else {
                        let (mut steal_size, mut steal_len) = (0, 0);
                        while steal_size < new.size() {
                            if pos >= class.fields.len() {
                                break;
                            }

                            let index = pos + steal_len;
                            if index >= class.fields.len() {
                                break;
                            }

                            steal_size += class.fields[index].size();
                            steal_len += 1;
                        }

                        if steal_size < new.size() {
                            state.toasts.error("Not enough space for a new field");
                        } else {
                            class.fields.drain(pos..pos + steal_len);
                            let mut padding = allocate_padding(steal_size - new.size());
                            class.fields.insert(pos, new.into_field(old_name));

                            while let Some(pad) = padding.pop() {
                                class.fields.insert(pos + 1, pad);
                            }

                            state.selection.as_mut().unwrap().field_id = class.fields[pos].id();
                        }
                    }

                    state.dummy = false;
                }
            }
            Some(ToolBarResponse::ProcessDetach) => {
                let mut state = self.state.borrow_mut();

                if let Some(mut process) = state
                    .process
                    .clone() /* ??? */
                    .try_write()
                {
                    *process = None;
                    frame.set_window_title("YClass");
                } else {
                    state.toasts.warning("Process is currently in use");
                }
            }
            Some(ToolBarResponse::ProcessAttach(pid)) => {
                let mut state = self.state.borrow_mut();

                if let Some(mut process) = state
                    .process
                    .clone() /* ??? */
                    .try_write()
                {
                    match Process::attach(pid, &state.config) {
                        Ok(proc) => {
                            frame.set_window_title(&format!("YClass - Attached to {pid}"));
                            if let Process::Internal((op, _)) = &proc {
                                match op.name() {
                                    Ok(name) => {
                                        state.config.last_attached_process_name = Some(name);
                                        state.config.save();
                                    }
                                    Err(e) => {
                                        _ = state
                                            .toasts
                                            .error(format!("Failed to get process name: {e}"))
                                    }
                                }
                            }

                            *process = Some(proc);
                        }
                        Err(e) => {
                            state.toasts.error(format!(
                                "Failed to attach to process.\nPossibly plugin error.\n{e}"
                            ));
                        }
                    }
                } else {
                    state.toasts.warning("Process is currently in use");
                }
            }
            None => {}
        }
    }
}

impl App for YClassApp {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        ctx.request_repaint_after(Duration::from_millis(100));

        static DPI_INIT: Once = Once::new();
        DPI_INIT.call_once(|| {
            let dpi = self.state.borrow().config.dpi.unwrap_or(1.);
            ctx.set_pixels_per_point(dpi);
        });

        let res = self.tool_bar.show(ctx);
        self.handle_reponse(frame, res);

        self.class_list.show(ctx);

        let res = self.inspector.show(ctx);
        self.handle_reponse(frame, res);

        let mut style = (*ctx.style()).clone();
        let saved = style.clone();
        style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(0x10, 0x10, 0x10);
        style.visuals.widgets.noninteractive.fg_stroke.color = Color32::LIGHT_GRAY;
        ctx.set_style(style);

        self.state.borrow_mut().toasts.show(ctx);
        ctx.set_style(saved);
    }
}

pub fn is_valid_ident(name: &str) -> bool {
    !name.starts_with(char::is_numeric) && !name.contains(char::is_whitespace) && !name.is_empty()
}
