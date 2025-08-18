use super::{
    create_text_format, display_field_prelude, next_id, CodegenData, Field, FieldId, FieldKind,
    FieldResponse,
};
use crate::{context::InspectionContext, generator::Generator};
use eframe::{
    egui::{Label, ScrollArea, Sense, Ui},
    epaint::{text::LayoutJob, Color32},
};
use once_cell::unsync::Lazy;
use std::{borrow::Cow, cell::RefCell, iter::repeat_with, ops::RangeFrom};

struct PreviewState {
    address: usize,
    hover_time: f32,
    shown: bool,
    offest: usize,
}

thread_local! {
    static PREVIEW_FIELDS: Lazy<Vec<Box<dyn Field>>> = Lazy::new(|| {
        repeat_with(|| Box::new(HexField::<8>::new()) as Box<dyn Field>)
            .take(20)
            .collect()
    });
}

impl PreviewState {
    fn new(address: usize) -> Self {
        Self {
            offest: 0,
            address,
            hover_time: 0.,
            shown: false,
        }
    }
}

pub struct HexField<const N: usize> {
    preview_state: RefCell<Option<PreviewState>>,
    id: FieldId,
}

impl<const N: usize> HexField<N> {
    pub fn new() -> Self {
        Self {
            id: next_id(),
            preview_state: None.into(),
        }
    }

    fn byte_view(&self, ctx: &mut InspectionContext, job: &mut LayoutJob, buf: &[u8; N]) {
        for (i, b) in buf.iter().enumerate() {
            let rng = fastrand::Rng::with_seed(*b as _);
            let color = if *b == 0 {
                Color32::DARK_GRAY
            } else {
                const MIN: RangeFrom<u8> = 45..;
                Color32::from_rgb(rng.u8(MIN), rng.u8(MIN), rng.u8(MIN))
            };

            job.append(
                &format!("{b:02X}"),
                4. + if i == 0 { 4. } else { 0. },
                create_text_format(ctx.is_selected(self.id), color),
            );
        }
    }

    fn int_view(&self, ui: &mut Ui, ctx: &mut InspectionContext, buf: &[u8; N]) {
        let mut job = LayoutJob::default();
        let (mut high, mut low) = (0i64, 0i64);

        let displayed = if N == 1 {
            buf[0] as i8 as i64
        } else {
            let half = N / 2;

            (high, low) = int_high_low_from_le::<N>(&buf[..half], &buf[half..]);

            match N {
                2 => i16::from_le_bytes(buf[..].try_into().unwrap()) as i64,
                4 => i32::from_le_bytes(buf[..].try_into().unwrap()) as i64,
                8 => i64::from_le_bytes(buf[..].try_into().unwrap()),
                _ => unreachable!(),
            }
        };

        job.append(
            &format!("{}", displayed),
            4.,
            create_text_format(ctx.is_selected(self.id), Color32::LIGHT_BLUE),
        );

        let r = ui.add(Label::new(job).sense(Sense::click()));
        if r.clicked() {
            ctx.select(self.id);
        }

        if N != 1 {
            r.on_hover_text(format!("High: {high}\nLow: {low}"));
        }
    }

    fn float_view(&self, ui: &mut Ui, ctx: &mut InspectionContext, buf: &[u8; N]) {
        if N != 4 && N != 8 {
            return;
        }

        let mut job = LayoutJob::default();

        let displayed = if N == 4 {
            f32::from_ne_bytes(buf[..].try_into().unwrap()) as f64
        } else {
            f64::from_ne_bytes(buf[..].try_into().unwrap())
        };

        job.append(
            &format!("{:e}", displayed),
            4.,
            create_text_format(ctx.is_selected(self.id), Color32::LIGHT_RED),
        );

        let r = ui.add(Label::new(job).sense(Sense::click()));
        if r.clicked() {
            ctx.select(self.id);
        }

        if N == 8 {
            let (high, low) = (
                f32::from_ne_bytes(buf[..4].try_into().unwrap()),
                f32::from_ne_bytes(buf[4..].try_into().unwrap()),
            );

            r.on_hover_text(format!("Full:{displayed}\nHigh: {high}\nLow: {low}"));
        } else if N == 4 {
            r.on_hover_text(format!("Full:{displayed}"));
        }
    }

    fn pointer_view(
        &self,
        ui: &mut Ui,
        ctx: &mut InspectionContext,
        buf: &[u8; N],
        response: &mut Option<FieldResponse>,
    ) {
        if N != 8 {
            return;
        }

        let address = usize::from_ne_bytes(buf[..].try_into().unwrap());
        if ctx.process.can_read(address) {
            let mut job = LayoutJob::default();
            job.append(
                &format!("-> {address:X}"),
                4.,
                create_text_format(ctx.is_selected(self.id), Color32::YELLOW),
            );

            let r = ui.add(Label::new(job).sense(Sense::click()));

            if r.clicked() {
                ctx.select(self.id);
            }

            let preview_state = &mut *self.preview_state.borrow_mut();
            if r.hovered() {
                if let Some(preview) = preview_state {
                    if preview.address == ctx.address + ctx.offset {
                        if !preview.shown {
                            ui.ctx().request_repaint();
                            preview.hover_time += ui.input(|i| i.stable_dt);
                            if preview.hover_time >= 0.3 {
                                preview.shown = true;
                                *response = Some(FieldResponse::LockScroll);
                            }
                        } else {
                            let yd = ui.input(|i| i.raw_scroll_delta.y);
                            if yd < 0. {
                                preview.offest = preview.offest.saturating_add(8);
                            } else if yd > 0. {
                                preview.offest = preview.offest.saturating_sub(8);
                            }

                            r.on_hover_ui(|ui| {
                                let saved = (ctx.address, ctx.offset);
                                ctx.address = address;
                                ctx.offset = preview.offest;

                                ScrollArea::vertical()
                                    .stick_to_bottom(true)
                                    .hscroll(false)
                                    .show(ui, |ui| {
                                        PREVIEW_FIELDS.with(|fields| {
                                            fields.iter().for_each(|f| _ = f.draw(ui, ctx));
                                        });
                                    });

                                (ctx.address, ctx.offset) = saved;
                            });
                        }
                    }
                } else {
                    *preview_state = Some(PreviewState::new(ctx.address + ctx.offset));
                }
            } else if let Some(preview) = preview_state {
                if preview.address == ctx.address + ctx.offset {
                    *preview_state = None;
                    *response = Some(FieldResponse::UnlockScroll);
                }
            }
        }
    }

    fn ascii_view(&self, ui: &mut Ui, ctx: &mut InspectionContext, buf: &[u8; N]) {
        let mut job = LayoutJob::default();

        for &byte in buf.iter() {
            let (color, ch) = if byte.is_ascii_graphic() || byte == b' ' {
                (Color32::LIGHT_GREEN, char::from(byte))
            } else {
                (Color32::DARK_GRAY, '.')
            };

            job.append(
                &ch.to_string(),
                0.,
                create_text_format(ctx.is_selected(self.id), color),
            );
        }

        let r = ui.add(Label::new(job).sense(Sense::click()));
        if r.clicked() {
            ctx.select(self.id);
        }
    }

    fn string_view(&self, ui: &mut Ui, ctx: &mut InspectionContext, buf: &[u8; N]) {
        if N != 8 {
            return;
        }

        let address = usize::from_ne_bytes(buf[..].try_into().unwrap());
        if ctx.process.can_read(address) {
            let mut str_buf = [0; 0x100];
            ctx.process.read(address, &mut str_buf);

            enum StrType {
                Str,
                WStr,
            }

            let str = {
                let len = str_buf
                    .chunks(2)
                    .position(|c| !(c[1] == 0 && char::from(c[0]).is_ascii_graphic()))
                    .unwrap_or(str_buf.len());

                if len > 5 {
                    let chars = str_buf
                        .chunks(2)
                        .map(|c| u16::from_le_bytes(c.try_into().unwrap()))
                        .take(len)
                        .collect::<Vec<_>>();
                    Some((StrType::Str, Cow::Owned(String::from_utf16_lossy(&chars))))
                } else {
                    let len = str_buf
                        .iter()
                        .position(|c| !char::from(*c).is_ascii_graphic())
                        .unwrap_or(str_buf.len());

                    if len > 5 {
                        Some((StrType::WStr, String::from_utf8_lossy(&str_buf[..len])))
                    } else {
                        None
                    }
                }
            };

            if let Some((t, str)) = str {
                let mut job = LayoutJob::default();
                job.append(
                    &format!(
                        "-> {}{str:?}",
                        match t {
                            StrType::Str => "",
                            StrType::WStr => "L",
                        }
                    ),
                    4.,
                    create_text_format(ctx.is_selected(self.id), Color32::RED),
                );

                let r = ui.add(Label::new(job).sense(Sense::click()));

                if r.clicked() {
                    ctx.select(self.id);
                }
            }
        }
    }
}

impl<const N: usize> Field for HexField<N> {
    fn id(&self) -> FieldId {
        self.id
    }

    fn size(&self) -> usize {
        N
    }

    fn name(&self) -> Option<String> {
        None
    }

    fn kind(&self) -> FieldKind {
        match N {
            1 => FieldKind::Unk8,
            2 => FieldKind::Unk16,
            4 => FieldKind::Unk32,
            8 => FieldKind::Unk64,
            _ => unreachable!(),
        }
    }

    fn draw(&self, ui: &mut Ui, ctx: &mut InspectionContext) -> Option<FieldResponse> {
        let mut buf = [0; N];
        ctx.process.read(ctx.address + ctx.offset, &mut buf);

        let mut response = None;

        ui.horizontal(|ui| {
            let mut job = LayoutJob::default();
            display_field_prelude(ui.ctx(), self, ctx, &mut job);
            self.byte_view(ctx, &mut job, &buf);

            if ui.add(Label::new(job).sense(Sense::click())).clicked() {
                ctx.select(self.id);
            }

            self.ascii_view(ui, ctx, &buf);
            self.int_view(ui, ctx, &buf);
            self.float_view(ui, ctx, &buf);
            self.pointer_view(ui, ctx, &buf, &mut response);
            self.string_view(ui, ctx, &buf);
        });

        ctx.offset += N;
        response
    }

    fn codegen(&self, generator: &mut dyn Generator, _: &CodegenData) {
        generator.add_offset(self.size());
    }
}

fn int_high_low_from_le<const N: usize>(high: &[u8], low: &[u8]) -> (i64, i64) {
    match N {
        8 => (
            i32::from_ne_bytes(high.try_into().unwrap()) as _,
            i32::from_ne_bytes(low.try_into().unwrap()) as _,
        ),
        4 => (
            i16::from_ne_bytes(high.try_into().unwrap()) as _,
            i16::from_ne_bytes(low.try_into().unwrap()) as _,
        ),
        2 => (
            i8::from_ne_bytes(high.try_into().unwrap()) as _,
            i8::from_ne_bytes(low.try_into().unwrap()) as _,
        ),
        _ => unreachable!(),
    }
}
