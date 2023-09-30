use super::{GeneratorWindow, ProcessAttachWindow, SpiderWindow};
use crate::{
    class::ClassList,
    field::FieldKind,
    state::{GlobalState, StateRef},
};
use eframe::{
    egui::{style::Margin, Button, Context, Frame, TopBottomPanel, Ui, WidgetText},
    epaint::Rounding,
};
use memflex::external::ProcessIterator;

pub enum ToolBarResponse {
    ProcessAttach(u32),
    ProcessDetach,
    Add(usize),
    Remove(usize),
    Insert(usize),
    ChangeKind(FieldKind),
}

pub struct ToolBarPanel {
    ps_attach_window: ProcessAttachWindow,
    generator_window: GeneratorWindow,
    spider_window: SpiderWindow,
    state: StateRef,
}

impl ToolBarPanel {
    pub fn new(state: StateRef) -> Self {
        Self {
            state,
            ps_attach_window: ProcessAttachWindow::new(state),
            generator_window: GeneratorWindow::new(state),
            spider_window: SpiderWindow::new(state),
        }
    }

    pub fn show(&mut self, ctx: &Context) -> Option<ToolBarResponse> {
        let mut response = None;

        if let Some(pid) = self.ps_attach_window.show(ctx) {
            response = Some(ToolBarResponse::ProcessAttach(pid));
            self.ps_attach_window.toggle();
        }

        self.generator_window.show(ctx);
        if let Err(e) = self.spider_window.show(ctx) {
            self.state.borrow_mut().toasts.error(e.to_string());
        }

        self.run_hotkeys(ctx, &mut response);

        let style = ctx.style();
        let frame = Frame {
            inner_margin: Margin::same(0.),
            rounding: Rounding::ZERO,
            fill: style.visuals.window_fill(),
            stroke: style.visuals.window_stroke(),
            ..Default::default()
        };

        TopBottomPanel::top("_top_bar")
            .frame(frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    ui.visuals_mut().widgets.inactive.rounding = Rounding::ZERO;

                    ui.menu_button("Project", |ui| self.project_menu(ui));
                    ui.menu_button("Process", |ui| self.process_menu(ui, &mut response));

                    if ui.button("Generator").clicked() {
                        self.generator_window.toggle();
                    }

                    if ui.button("Spider").clicked() {
                        self.spider_window.toggle();
                    }

                    ui.add_space(4.);
                    ui.separator();
                    ui.add_space(4.);

                    self.status_ui(ui, &mut response);
                });
            });

        response
    }

    fn run_hotkeys(&mut self, ctx: &Context, response: &mut Option<ToolBarResponse>) {
        let state = &mut *self.state.borrow_mut();
        let input = &ctx.input(|i| i.clone());

        if state.hotkeys.pressed("attach_process", input) {
            self.ps_attach_window.toggle();
        }

        if state.hotkeys.pressed("attach_recent", input) {
            if let Some(name) = state.config.last_attached_process_name.as_ref().cloned() {
                attach_to_process(state, &name, response);
            }
        }
    }

    fn project_menu(&mut self, ui: &mut Ui) {
        let state = &mut *self.state.borrow_mut();

        if ui.button("New project").clicked() {
            state.save_project(None);
            state.class_list = ClassList::default();
            ui.close_menu();
        }

        if ui.button("Open project").clicked() {
            state.open_project();
            ui.close_menu();
        }

        if !state
            .config
            .recent_projects
            .as_ref()
            .map(|h| h.is_empty())
            .unwrap_or(true)
        {
            ui.menu_button("Open recent...", |ui| {
                let mut to_open = None;
                for project in state.config.recent_projects.as_ref().unwrap().iter() {
                    if let Some(name) = project.file_name().and_then(|name| name.to_str()) {
                        if ui.button(name).clicked() {
                            to_open = Some(project.to_owned());
                        }
                    }
                }

                if let Some(path) = to_open {
                    if state.open_project_path(&path) {
                        ui.close_menu();
                    } else {
                        state.config.recent_projects.as_mut().unwrap().remove(&path);
                    }
                }
            });
        }

        if ui.button("Save project").clicked() {
            state.save_project(None);
            ui.close_menu();
        }

        if ui.button("Save project as").clicked() {
            state.save_project_as();
            ui.close_menu();
        }
    }

    fn process_menu(&mut self, ui: &mut Ui, response: &mut Option<ToolBarResponse>) {
        ui.set_width(200.);

        let state = &mut *self.state.borrow_mut();

        if shortcut_button(ui, state, "attach_process", "Attach to process") {
            self.ps_attach_window.toggle();
            ui.close_menu();
        }

        // Reattach to last process
        if let Some(name) = state.config.last_attached_process_name.as_ref().cloned() {
            if shortcut_button(ui, state, "attach_recent", format!("Attach to {name}")) {
                attach_to_process(state, &name, response);

                ui.close_menu();
            }
        }

        if shortcut_button(ui, state, "detach_process", "Detach from process") {
            *response = Some(ToolBarResponse::ProcessDetach);
            ui.close_menu();
        }
    }

    fn status_ui(&mut self, ui: &mut Ui, response: &mut Option<ToolBarResponse>) {
        if let Some((proc_name, proc_id)) = self
            .state
            .borrow()
            .process
            .as_ref()
            .read()
            .as_ref()
            .map(|p| (p.name(), p.id()))
        {
            match proc_name {
                Ok(name) => _ = ui.label(format!("Status: Attached to {} - {}", name, proc_id)),
                Err(e) => {
                    self.state
                        .borrow_mut()
                        .toasts
                        .error(format!("Failed to get process name: {e}"));
                    *response = Some(ToolBarResponse::ProcessDetach);
                }
            };
        } else {
            ui.label("Status: Detached");
        }
    }
}

fn shortcut_button(
    ui: &mut Ui,
    state: &GlobalState,
    name: &'static str,
    label: impl Into<WidgetText>,
) -> bool {
    ui.add(Button::new(label).shortcut_text(state.hotkeys.format(name, ui.ctx())))
        .clicked()
}

fn attach_to_process(state: &mut GlobalState, name: &str, response: &mut Option<ToolBarResponse>) {
    let last_proc = match ProcessIterator::new() {
        Ok(mut piter) => piter.find(|pe| pe.name.eq_ignore_ascii_case(name)),
        Err(e) => {
            state
                .toasts
                .error(format!("Failed to iterate over processes. {e}"));
            return;
        }
    };

    if let Some(pe) = last_proc {
        *response = Some(ToolBarResponse::ProcessAttach(pe.id));
    } else {
        state.toasts.error(format!("Failed to find {name}"));
    }
}
