use eframe::{
    egui::{Label, RichText, Sense},
    epaint::{text::LayoutJob, Color32},
};

use crate::FID_M;

use super::{
    display_field_name, display_field_prelude, display_field_value, next_id, Field, FieldId,
    FieldKind, NamedState,
};

pub struct StringPointerField {
    id: FieldId,
    state: NamedState,
}

impl StringPointerField {
    pub fn new(name: String) -> Self {
        Self {
            id: next_id(),
            state: NamedState::new(name),
        }
    }
}

impl Field for StringPointerField {
    fn id(&self) -> FieldId {
        self.id
    }

    fn name(&self) -> Option<String> {
        Some(self.state.name.borrow().clone())
    }

    fn size(&self) -> usize {
        // TODO: The size of the pointer would be 4 bytes on x86
        8
    }

    fn kind(&self) -> super::FieldKind {
        FieldKind::StrPtr
    }

    fn draw(
        &self,
        ui: &mut eframe::egui::Ui,
        ctx: &mut crate::context::InspectionContext,
    ) -> Option<super::FieldResponse> {
        // TODO: The size of the pointer would be 4 bytes on x86
        let mut buf = [0; 8];
        ctx.process.read(ctx.address + ctx.offset, &mut buf);
        let address = usize::from_ne_bytes(buf);

        let mut str_buf = [0; 64];
        ctx.process.read(address, &mut str_buf);

        ui.horizontal(|ui| {
            let mut job = LayoutJob::default();
            display_field_prelude(ui.ctx(), self, ctx, &mut job);
            if ui.add(Label::new(job).sense(Sense::click())).clicked() {
                ctx.select(self.id);
            }
            display_field_name(self, ui, ctx, &self.state, Color32::LIGHT_RED);
            if ctx.process.can_read(address) {
                display_field_value(
                    self,
                    ui,
                    ctx,
                    &self.state,
                    Color32::LIGHT_BLUE,
                    |v| {
                        let str_end = str_buf
                            .iter()
                            .position(|c| *c == b'\0')
                            .unwrap_or(str_buf.len());
                        let str = std::string::String::from_utf8_lossy(&str_buf[..str_end]);

                        if v {
                            format!("{str:?}")
                        } else {
                            format!("-> {str:?}")
                        }
                    },
                    |_| false,
                )
            } else {
                ui.add_space(2.);
                ui.heading(
                    RichText::new("Invalid Address")
                        .color(Color32::RED)
                        .font(FID_M),
                );
            }
        });
        ctx.offset += self.size();
        None
    }

    fn codegen(&self, generator: &mut dyn crate::generator::Generator, _: &super::CodegenData) {
        generator.add_field(self.state.name.borrow().as_str(), FieldKind::StrPtr, None);
    }
}

pub struct WideStringPointerField {
    id: FieldId,
    state: NamedState,
}

impl WideStringPointerField {
    pub fn new(name: String) -> Self {
        Self {
            id: next_id(),
            state: NamedState::new(name),
        }
    }
}

impl Field for WideStringPointerField {
    fn id(&self) -> FieldId {
        self.id
    }

    fn name(&self) -> Option<String> {
        Some(self.state.name.borrow().clone())
    }

    fn size(&self) -> usize {
        // TODO: The size of the pointer would be 4 bytes on x86
        8
    }

    fn kind(&self) -> super::FieldKind {
        FieldKind::StrPtr
    }

    fn draw(
        &self,
        ui: &mut eframe::egui::Ui,
        ctx: &mut crate::context::InspectionContext,
    ) -> Option<super::FieldResponse> {
        // TODO: The size of the pointer would be 4 bytes on x86
        let mut buf = [0; 8];
        ctx.process.read(ctx.address + ctx.offset, &mut buf);
        let address = usize::from_ne_bytes(buf);

        let mut str_buf = [0; 64];
        ctx.process.read(address, &mut str_buf);

        ui.horizontal(|ui| {
            let mut job = LayoutJob::default();
            display_field_prelude(ui.ctx(), self, ctx, &mut job);
            if ui.add(Label::new(job).sense(Sense::click())).clicked() {
                ctx.select(self.id);
            }
            display_field_name(self, ui, ctx, &self.state, Color32::LIGHT_RED);
            if ctx.process.can_read(address) {
                display_field_value(
                    self,
                    ui,
                    ctx,
                    &self.state,
                    Color32::LIGHT_BLUE,
                    |v| {
                        let chars = str_buf
                            .chunks(2)
                            .map(|c| u16::from_le_bytes(c.try_into().unwrap()))
                            .take_while(|c| *c != 0)
                            .collect::<Vec<_>>();
                        let str = String::from_utf16_lossy(&chars);

                        if v {
                            format!("{str:?}")
                        } else {
                            format!("-> {str:?}")
                        }
                    },
                    |_| false,
                )
            } else {
                ui.add_space(2.);
                ui.heading(
                    RichText::new("Invalid Address")
                        .color(Color32::RED)
                        .font(FID_M),
                );
            }
        });
        ctx.offset += self.size();
        None
    }

    fn codegen(&self, generator: &mut dyn crate::generator::Generator, _: &super::CodegenData) {
        generator.add_field(self.state.name.borrow().as_str(), FieldKind::StrPtr, None);
    }
}
