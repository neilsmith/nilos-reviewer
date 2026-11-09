//! rune-fx compositor: effect slots become fx layers. One mark, GLOW —
//! a breathing accent disc for loading states. `animating()` reports true
//! only while an effect slot is mounted, so the window still idles.

use rune::compose::{Compositor, Frame};
use rune::render::{EffectPhase, EffectSite, KIND_EFFECT};
use rune_fx::*;

use crate::app::App;

pub const GLOW: u32 = 1;

/// Glow colours indexed by `effect_data[0]` (0 = accent, 1 = success).
const GLOW_COLORS: [u32; 2] = [0x4f8dff, 0x34d399];

fn layer(kind: u32, d: [f32; 4], shape: Shape, t: f32) -> Option<FxLayer> {
    if kind != GLOW {
        return None;
    }
    let color = rgb(GLOW_COLORS[(d[0].max(0.0) as usize).min(GLOW_COLORS.len() - 1)]);
    Some(
        Fx::glow(GlowParams {
            color,
            core: color,
            radius: if d[3] > 0.0 { d[3] } else { 14.0 },
            intensity: if d[1] > 0.0 { d[1] } else { 1.0 },
            breathe: if d[2] > 0.0 { d[2] } else { 0.45 },
            breathe_period: 2.4,
            ..Default::default()
        })
        .on(shape)
        .at(t),
    )
}

#[derive(Default)]
pub struct Gfx {
    fx: Option<FxRenderer>,
    /// True while the last frame mounted ≥1 effect slot.
    active: bool,
}

impl Compositor<App> for Gfx {
    fn animating(&self) -> bool {
        self.active
    }

    fn render(&mut self, f: &mut Frame<'_, App>) {
        let (device, queue, size) = (f.device, f.queue, f.size);
        let t = f.rt.t as f32;
        let mut at: Vec<u32> = Vec::new();
        let mut layers: Vec<FxLayer> = Vec::new();
        let mut placements: Vec<FxSlot> = Vec::new();
        for (n, i) in f.rt.instances.iter().enumerate() {
            if i.params[1] != KIND_EFFECT {
                continue;
            }
            let kind = f.rt.marks.get(n).copied().unwrap_or(0) & 0xff;
            let k = i.params[3];
            let [x, y, w, h] = i.rect.map(|v| v / k);
            let r = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / k;
            let Some(l) = layer(kind, i.uv, Shape::rounded(x, y, w, h, r), t) else {
                continue;
            };
            at.push(n as u32);
            layers.push(l);
            placements.push(FxSlot::default().scale(k).clip(i.clip, i.fade));
        }
        self.active = !layers.is_empty();
        let fx = self
            .fx
            .get_or_insert_with(|| FxRenderer::new(device, f.format, FxOptions { dither: true }));
        fx.set_srgb_blend(f.srgb_blend());
        fx.add_format(device, wgpu::TextureFormat::Rgba16Float);
        fx.prepare_with_slots(
            device,
            queue,
            &FxFrame {
                viewport: [size.0, size.1],
                scale: 1.0,
                layers: &layers,
                backdrop: None,
                sources: &[],
            },
            &placements,
        );
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        f.ui.draw_with_backdrop(
            f.encoder,
            f.target,
            clear,
            &mut |phase, s: &EffectSite<'_>| {
                let Some(k) = at.iter().position(|n| *n == s.index) else {
                    return;
                };
                if let EffectPhase::Draw(pass) = phase {
                    fx.draw_layer_in(pass, s.format, s.origin, k);
                }
            },
        );
    }
}
