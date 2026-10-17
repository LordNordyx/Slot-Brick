use std::time::{Duration, Instant};

use slot_gfx::{Compositor, Draw, ScreenEffect, TexId, OUT_H, OUT_W};

const PAPER: &[u8] = include_bytes!("../assets/paper.png");
use slot_input::{InputSource, Millis};
use slot_power::{Platform, Power};
use slot_store::format_stamp;
use slot_ui::{
    arrows_hint_face, badge_face, bezel_face, cart_shadow, chip_face, chip_shadow_face,
    date_time_text, gb_cart_shadow, hhmm, hint_face, icon_face, menu_face, photo_face,
    quick_caret_face, quick_label_face, quick_legend_faces, quick_value_face, rebuild_count_face,
    rebuild_title_face, set_clock_hint_face, socket_face, sticker_face, title_face, toast_face,
    wallpaper_face, word_face, GbShell, Icon, LinkBadge, PowerChoice, QuickMenuFaces, QuickRow,
    QuickValue, RebuildScreen, StickerFields, Toast, UndoFace, ALERT_PX, BOLT_PX, HUD_ICON_PX,
    HUD_INK, LEGEND,
};

use crate::app::{App, LinkRow, Phase};
use crate::build_info::Build;
use crate::cart_faces::CartFaces;
use crate::face_builder::FaceBuilder;
use crate::label_cache::{self, Rebuild};
use crate::link_art_builder::LinkArtBuilder;
use crate::link_screen::{LinkSprites, Sprite};
use crate::link_start::{LinkFail, LinkStep};
use crate::session::Session;
use crate::wallpaper;

const DOZE_TIMEOUT: Duration = Duration::from_secs(180);
// The Brick has no lid, so an untouched device would otherwise stay lit until
// the battery is flat. Idle limits before slot dozes (and then suspends).
#[cfg(feature = "brick")]
const IDLE_MENU: Duration = Duration::from_secs(5 * 60);
#[cfg(feature = "brick")]
const IDLE_GAME: Duration = Duration::from_secs(15 * 60);
// After a wake nobody follows up with input (charger plug, bumped key), sleep again soon.
#[cfg(feature = "brick")]
const IDLE_AFTER_WAKE: Duration = Duration::from_secs(90);

const ALERT_INK: [u8; 3] = [0xf0, 0xb4, 0x3c];

pub struct Frontend {
    session: Session,
    start: Instant,
    last: Instant,
    wake_guard: Option<Instant>,
    last_input: Instant,
    last_render: Instant,
    rendered_effects: (f32,f32,f32,Option<f32>),
    draws: Vec<Draw>,
    polaroid_texes: Vec<TexId>,
    title_tex: Option<TexId>,
    faces: FaceBuilder,
    /// The label row's words, [dim, lit], rewritten as the label search goes.
    /// The label grid's textures, kept and written over as its pages turn.
    label_grid_tex: LabelGridTex,
    label_adjust_tex: LabelAdjustTex,
    peel_tex: PeelTex,
    cart_faces: CartFaces,
    rebuild: Option<Rebuild>,
    rebuild_faces: RebuildFaces,
    link_art: LinkArtBuilder,
    link_art_done: bool,
    core_asked: Option<String>,
    core_board_tex: Option<TexId>,
    core_lid_tex: Option<TexId>,
    core_built: Option<String>,
    undo_tex: Option<TexId>,
    switcher: Switcher,
    clocks: Clocks,
    about: AboutFace,
    quick_clock: QuickClock,
    #[cfg(feature = "brick")]
    wifi_tex: Option<TexId>,
    banner_tex: Option<TexId>,
    #[cfg(feature = "brick")]
    auto_sleep: bool,
    #[cfg(feature = "brick")]
    woke_untouched: bool,
}

#[derive(Default)]
struct RebuildFaces {
    title: Option<(TexId, u32, u32)>,
    count: Option<TexId>,
    count_size: (u32, u32),
    shown: Option<usize>,
}

#[derive(Default)]
struct QuickClock {
    dim: Option<TexId>,
    lit: Option<TexId>,
    shown: String,
}

#[derive(Default)]
struct AboutFace {
    tex: Option<TexId>,
    battery: Option<u8>,
}

#[derive(Default)]
struct Clocks {
    line: Option<TexId>,
    hint: Option<TexId>,
    shelf: Option<TexId>,
    picked: Option<String>,
    shown: String,
    battery: String,
    battery_tex: Option<TexId>,
    platform: String,
    platform_tex: Option<TexId>,
}

#[derive(Default)]
struct Switcher {
    open: bool,
    titled: Option<String>,
}

impl Frontend {
    pub fn boot(platform: Box<dyn Platform>) -> Self {
        let now = Instant::now();
        let cart_faces = CartFaces::spawn(platform.root().to_path_buf());
        #[cfg(feature = "brick")]
        let session_root = platform.root().to_path_buf();
        let mut session = Session::boot(platform.root().to_path_buf());
        label_cache::forget_old(platform.root());
        let stale = label_cache::stale(platform.root(), session.app().carts());
        let rebuild = (!stale.is_empty()).then(|| {
            eprintln!("slot: label cache: {} new labels", stale.len());
            Rebuild::start(platform.root(), stale)
        });
        session
            .app_mut()
            .set_power(Power::new(platform, DOZE_TIMEOUT));
        Frontend {
            session,
            start: now,
            last: now,
            wake_guard: None,
            last_input: now,
            last_render: now,
            rendered_effects: (0.0,0.0,0.0,None),
            draws: Vec::new(),
            polaroid_texes: Vec::new(),
            title_tex: None,
            faces: FaceBuilder::spawn(),
            label_grid_tex: LabelGridTex::default(),
            label_adjust_tex: LabelAdjustTex::default(),
            peel_tex: PeelTex::default(),
            cart_faces,
            rebuild,
            rebuild_faces: RebuildFaces::default(),
            link_art: LinkArtBuilder::spawn(),
            link_art_done: false,
            core_asked: None,
            core_board_tex: None,
            core_lid_tex: None,
            core_built: None,
            undo_tex: None,
            switcher: Switcher::default(),
            clocks: Clocks::default(),
            about: AboutFace::default(),
            quick_clock: QuickClock::default(),
            #[cfg(feature = "brick")]
            wifi_tex: None,
            banner_tex: None,
            #[cfg(feature = "brick")]
            auto_sleep: !platform_root_has(&session_root, "DISABLE_AUTO_SLEEP"),
            #[cfg(feature = "brick")]
            woke_untouched: false,
        }
    }

    fn upload_paper(compositor: &mut Compositor) {
        let mut dec = png::Decoder::new(std::io::Cursor::new(PAPER));
        dec.set_transformations(png::Transformations::normalize_to_color8());
        let Ok(mut reader) = dec.read_info() else {
            return;
        };
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let Ok(info) = reader.next_frame(&mut buf) else {
            return;
        };
        if info.color_type != png::ColorType::Grayscale || info.width != info.height {
            return;
        }
        let rgba: Vec<u8> = buf[..info.buffer_size()]
            .iter()
            .flat_map(|&l| [l, l, l, 255])
            .collect();
        compositor.set_paper(info.width, &rgba);
    }

    pub fn upload_faces(&mut self, compositor: &mut Compositor) {
        Self::upload_paper(compositor);
        #[cfg(feature = "brick")]
        slot_ui::upload_slot_art(|w,h,rgba|compositor.create_texture(w,h,rgba));
        if self.rebuild.is_none() {
            self.cart_faces.sync(self.session.app_mut(), compositor);
        }
        let icons = Icon::ALL
            .iter()
            .map(|i| {
                let f = icon_face(*i, HUD_ICON_PX, HUD_INK);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_icon_faces(icons);
        let link_badges = LinkBadge::FACES
            .iter()
            .map(|b| {
                let (badge, ink) = (
                    b.badge().expect("a face has a glyph"),
                    b.colour().expect("and a colour"),
                );
                let f = badge_face(badge, HUD_ICON_PX, ink);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_link_badge_faces(link_badges);
        let alert = icon_face(Icon::Alert, ALERT_PX, ALERT_INK);
        let alert = compositor.create_texture(alert.w, alert.h, &alert.rgba);
        self.session.app_mut().set_alert_face(alert);
        let lines = PowerChoice::ALL
            .iter()
            .map(|c| {
                let f = menu_face(match c {
                    PowerChoice::Restart => {
                        if cfg!(feature = "brick") {
                            "Saving and Exiting"
                        } else {
                            "Restarting"
                        }
                    }
                    PowerChoice::PowerOff => {
                        if cfg!(feature = "brick") {
                            "Saving and Shutting Down"
                        } else {
                            "Powering Down"
                        }
                    }
                });
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        self.session.app_mut().set_shutdown_faces(lines);
        let menu = PowerChoice::ALL
            .iter()
            .map(|c| {
                let f = menu_face(c.text());
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            })
            .collect();
        self.session.app_mut().set_power_menu_faces(menu);
        let mut up = |f: UndoFace| (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h);
        let labels = QuickRow::ALL
            .iter()
            .map(|r| up(quick_label_face(*r)))
            .collect();
        let values = QuickValue::ALL
            .iter()
            .map(|v| [false, true].map(|lit| up(quick_value_face(v.text(), lit))))
            .collect();
        let carets = [false, true].map(|right| up(quick_caret_face(right)));
        let legend = quick_legend_faces().map(|f| {
            let (tex, w, _) = up(f);
            (tex, w)
        });
        self.session.app_mut().set_quick_menu_faces(QuickMenuFaces {
            labels,
            values,
            carets,
            legend,
        });
        self.upload_cart_menu_faces(compositor);
        let sockets = slot_store::Core::PICKER
            .iter()
            .map(|c| {
                let f = socket_face(*c);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        let chips = slot_store::Core::PICKER
            .iter()
            .map(|c| {
                let f = chip_face(Some(*c));
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        let blank = chip_face(None);
        let blank = compositor.create_texture(blank.w, blank.h, &blank.rgba);
        let shadow = chip_shadow_face();
        let shadow = compositor.create_texture(shadow.w, shadow.h, &shadow.rgba);
        self.session
            .app_mut()
            .set_core_part_faces(sockets, chips, blank, shadow);
        let legend = [
            hint_face("B", "Cancel"),
            arrows_hint_face("Core"),
            slot_ui::caret_hint_face(true, "Colour"),
            slot_ui::caret_hint_face(false, "Finish"),
            hint_face("A", "Choose"),
        ]
        .into_iter()
        .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
        .collect();
        self.session.app_mut().set_core_legend_faces(legend);
        // Y sits on its own, above the legend rather than in it: it is about the cart's label
        // and not about the core being picked.
        let search = hint_face("Y", "Search");
        let face = (compositor.create_texture(search.w, search.h, &search.rgba), search.w);
        self.session.app_mut().set_core_search_face(face);
        let roles = menu_faces(compositor, LinkRow::ALL.iter().map(|r| r.text()));
        self.session.app_mut().set_link_menu_faces(roles);
        if let Some(linked) = menu_faces(compositor, ["Linked"].into_iter()).pop() {
            self.session.app_mut().set_link_linked_face(linked);
        }
        let legend = [
            hint_face("B", "Cancel"),
            hint_face("SELECT", "Mode"),
            arrows_hint_face("Swap"),
            hint_face("A", "Link"),
            hint_face("A", "OK"),
            hint_face("B", "Back"),
            hint_face("A", "End Link"),
        ]
        .into_iter()
        .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
        .collect();
        self.session.app_mut().set_link_legend_faces(legend);
        let steps = menu_faces(compositor, LinkStep::ALL.iter().map(|s| s.line()));
        self.session.app_mut().set_link_step_faces(steps);
        let fails = menu_faces(compositor, LinkFail::SHOWN.iter().map(|f| f.line()));
        self.session.app_mut().set_link_fail_faces(fails);
        let toasts = Toast::all()
            .into_iter()
            .map(|t| {
                let f = toast_face(t);
                compositor.create_texture(f.w, f.h, &f.rgba)
            })
            .collect();
        self.session.app_mut().set_toast_faces(toasts);
        let legend = legend_faces(compositor, &LEGEND);
        self.session.app_mut().set_legend_faces(legend);
        let hint = set_clock_hint_face();
        self.clocks.hint = Some(compositor.create_texture(hint.w, hint.h, &hint.rgba));
        let shadow = cart_shadow();
        let id = compositor.create_texture(shadow.w, shadow.h, &shadow.rgba);
        self.session.app_mut().set_cart_shadow(id);
        let notched = gb_cart_shadow(GbShell::Notched);
        let notched = compositor.create_texture(notched.w, notched.h, &notched.rgba);
        let rounded = gb_cart_shadow(GbShell::Rounded);
        let rounded = compositor.create_texture(rounded.w, rounded.h, &rounded.rgba);
        self.session.app_mut().set_gb_cart_shadows(notched, rounded);
        let [nes, famicom] = [slot_ui::NesShell::Nes, slot_ui::NesShell::Famicom].map(|shell| {
            let face = slot_ui::nes_cart_shadow(shell);
            compositor.create_texture(face.w, face.h, &face.rgba)
        });
        self.session.app_mut().set_nes_cart_shadows(nes, famicom);
        let [us, pal] = [slot_ui::SnesShell::Us, slot_ui::SnesShell::Pal].map(|shell| {
            let face = slot_ui::snes_cart_shadow(shell);
            compositor.create_texture(face.w, face.h, &face.rgba)
        });
        self.session.app_mut().set_snes_cart_shadows(us, pal);
        let bolt = icon_face(Icon::Charging, BOLT_PX, HUD_INK);
        let bolt_id = compositor.create_texture(bolt.w, bolt.h, &bolt.rgba);
        self.session.app_mut().set_bolt_face(bolt_id);
        // The shelf's Wi-Fi symbol, in its three states; which one shows is the app's to say.
        let wifi = [Icon::Wifi, Icon::WifiNoRoute, Icon::WifiOff].map(|icon| {
            let face = icon_face(icon, slot_ui::WIFI_PX, HUD_INK);
            (compositor.create_texture(face.w, face.h, &face.rgba), face.w, face.h)
        });
        self.session.app_mut().set_wifi_faces(wifi);
        self.upload_wallpaper(compositor);
    }

    /// The cart menu's words, each value dim and lit: built once, as the quick menu's are.
    fn upload_cart_menu_faces(&mut self, compositor: &mut Compositor) {
        use slot_ui::{cart_menu_label_face, cart_menu_value_face, CartRow};
        let mut up = |f: UndoFace| (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h);
        let labels = CartRow::ALL.iter().map(|r| up(cart_menu_label_face(*r))).collect();
        let mut values = |names: Vec<&str>| -> Vec<[(TexId, u32, u32); 2]> {
            names
                .into_iter()
                .map(|n| [false, true].map(|lit| up(cart_menu_value_face(n, lit))))
                .collect()
        };
        let mut colour_names: Vec<&str> = slot_ui::colour_choices().iter().map(|(n, _)| *n).collect();
        // A colour picked elsewhere (Cart Studio) shows as Custom, after all the named ones.
        colour_names.push("Custom");
        let colours = values(colour_names);
        let finishes = values(slot_ui::FINISHES.iter().map(|f| slot_ui::finish_name(*f)).collect());
        let shapes = values(
            slot_ui::SHAPES.iter().chain(&slot_ui::SNES_SHAPES).chain(&slot_ui::NES_SHAPES).copied().collect(),
        );
        let carets = [false, true].map(|right| up(quick_caret_face(right)));
        let legend = slot_ui::cart_menu_legend_faces().map(|f| {
            let (tex, w, _) = up(f);
            (tex, w)
        });
        self.session.app_mut().set_cart_menu_faces(slot_ui::CartMenuFaces {
            labels,
            colours,
            finishes,
            shapes,
            carets,
            legend,
        });
    }

    fn upload_wallpaper(&mut self, compositor: &mut Compositor) {
        let app = self.session.app();
        let seed = app.wall_secs().unsigned_abs();
        let Some(rgba) = app
            .root()
            .and_then(|root| wallpaper::pick(root, seed))
            .and_then(|path| wallpaper_face(&path))
        else {
            return;
        };
        let id = compositor.create_texture(OUT_W, OUT_H, &rgba);
        self.session.app_mut().set_wallpaper(id);
    }

    pub fn upload_bezel(&mut self, compositor: &mut Compositor, panel: (u32, u32)) {
        if !slot_gfx::framed(panel) {
            return;
        }
        slot_ui::set_shelf_slack((OUT_W * panel.1 / panel.0).saturating_sub(OUT_H) as f32);
        let Some(path) = self
            .session
            .app()
            .root()
            .and_then(|root| crate::bezel::pick(root, panel))
        else {
            return;
        };
        if let Some(rgba) = bezel_face(&path, panel.0, panel.1) {
            compositor.set_bezel(panel.0, panel.1, &rgba);
            eprintln!("slot: bezel {}", path.display());
        }
    }

    #[cfg(feature = "brick")]
    pub fn power_mode(&self) -> crate::brick_power::Mode {
        use crate::brick_power::Mode;
        if matches!(self.app().phase(),Phase::Doze{..}) {return Mode::Standby}
        if self.rebuild.is_some() || self.app().link_active() || self.session.actually_fast_forwarding() || self.session.actually_rewinding() {return Mode::Boost}
        match self.app().phase() {
            Phase::Playing{..} if self.session.observed_speed()==Some(crate::emu::Speed::Normal)=>Mode::Game,
            Phase::Shelf|Phase::QuickMenu{..} if self.last_input.elapsed()>Duration::from_secs(2)=>Mode::Menu,
            _=>Mode::Boost,
        }
    }
    #[cfg(feature = "brick")]
    pub fn audio_underruns(&self)->u64 {self.session.audio_ring().underruns()}
    #[cfg(feature = "brick")]
    pub fn needs_render(&self)->bool {
        if self.app().wifi.as_ref().map(|w|w.dirty).unwrap_or(false) {return true}
        if self.app().banner_active() {return true}
        if self.render_effects()!=self.rendered_effects {return true}
        if self.rebuild.is_some() || !matches!(self.app().phase(),Phase::Shelf|Phase::QuickMenu{..}) || self.last_input.elapsed()<Duration::from_secs(2) || self.last_render.elapsed()>=Duration::from_secs(1) {return true}
        let mut probe=Vec::new();self.app().draw(&mut probe);
        probe!=self.draws
    }

    fn render_effects(&self)->(f32,f32,f32,Option<f32>) {
        (self.app().screen_power(),self.app().screen_shake(),self.app().frame_lift(),self.app().frame_split())
    }
    pub fn render(&mut self, compositor: &mut Compositor, window: (u32, u32)) {
        self.last_render=Instant::now();
        self.rendered_effects=self.render_effects();
        compositor.fit(window);
        self.compose(compositor);
        compositor.end_frame(window);
    }

    pub fn compose(&mut self, compositor: &mut Compositor) {
        compositor.set_blue_light(self.session.app().blue_light());
        compositor.set_picture(self.session.app().picture_rect());
        compositor.set_screen_effect(match self.session.app().screen_shader() {
            slot_store::Shader::Off => ScreenEffect::None,
            slot_store::Shader::Lcd3x => ScreenEffect::Lcd3x,
            slot_store::Shader::Grid => ScreenEffect::Grid,
            slot_store::Shader::Dot => ScreenEffect::Dot,
            slot_store::Shader::Simpletex => ScreenEffect::Simpletex,
        });
        compositor.set_shake(self.session.app().screen_shake());
        compositor.set_screen_power(self.session.app().screen_power());
        compositor.set_frame_lift(self.session.app().frame_lift());
        compositor.set_frame_split(self.session.app().frame_split());
        let (cw, ch) = self.session.app().canvas();
        compositor.set_game_canvas(cw, ch);
        compositor.set_game_source_rect(self.session.app().source_rect());
        compositor.begin_frame();
        if self.draw_rebuild(compositor) {
            return;
        }
        if let Some(frame) = self.session.frame() {
            compositor.upload_game(&frame);
            crate::latency::taken();
        }
        self.cart_faces.sync(self.session.app_mut(), compositor);
        #[cfg(feature = "brick")]
        if let Some(wifi)=&mut self.session.app_mut().wifi {
            if wifi.dirty {
                let f=wifi.render();
                let tex=match self.wifi_tex {Some(id)=>{compositor.update_texture(id,f.w,f.h,&f.rgba);id},None=>compositor.create_texture(f.w,f.h,&f.rgba)};
                self.wifi_tex=Some(tex);wifi.face=Some(tex);wifi.dirty=false;
            }
        }
        if let Some((id, title, detail, badge)) = self.session.app().banner_to_render() {
            let px = slot_ui::BADGE_PX;
            let badge = badge.and_then(|p| slot_ui::cover(&p, px, px));
            let f = slot_ui::banner_face(&title, &detail, badge.as_deref());
            let tex = match self.banner_tex {
                Some(t) => {
                    compositor.update_texture(t, f.w, f.h, &f.rgba);
                    t
                }
                None => compositor.create_texture(f.w, f.h, &f.rgba),
            };
            self.banner_tex = Some(tex);
            self.session.app_mut().set_banner_face(id, tex);
        }
        sync_clock(self.session.app_mut(), compositor, &mut self.clocks);
        sync_about(self.session.app_mut(), compositor, &mut self.about);
        sync_quick_clock(self.session.app_mut(), compositor, &mut self.quick_clock);
        sync_label_grid(self.session.app_mut(), compositor, &mut self.label_grid_tex);
        sync_label_adjust(self.session.app_mut(), compositor, &mut self.label_adjust_tex);
        sync_peel(self.session.app_mut(), compositor, &mut self.peel_tex);
        sync_core_picker(
            self.session.app_mut(),
            compositor,
            &self.faces,
            &mut self.core_asked,
            &mut self.core_board_tex,
            &mut self.core_lid_tex,
            &mut self.core_built,
        );
        if !self.link_art_done {
            if let Some(art) = self.link_art.take() {
                let mut up = |f: &slot_ui::CartFace| Sprite {
                    tex: compositor.create_texture(f.w, f.h, &f.rgba),
                    w: f.w,
                    h: f.h,
                };
                let sprites = LinkSprites {
                    port: up(&art.port),
                    plug_host: up(&art.plug_host),
                    plug_join: up(&art.plug_join),
                    adapter: up(&art.adapter),
                    arcs_right: [
                        up(&art.arcs_right[0]),
                        up(&art.arcs_right[1]),
                        up(&art.arcs_right[2]),
                    ],
                    arcs_left: [
                        up(&art.arcs_left[0]),
                        up(&art.arcs_left[1]),
                        up(&art.arcs_left[2]),
                    ],
                    clicks: up(&art.clicks),
                    arrow_left: up(&art.arrow_left),
                    arrow_right: up(&art.arrow_right),
                };
                self.session.app_mut().set_link_sprites(sprites);
                self.link_art_done = true;
            }
        }
        sync_switcher(
            self.session.app_mut(),
            compositor,
            Faces {
                pool: &mut self.polaroid_texes,
                title: &mut self.title_tex,
                undo: &mut self.undo_tex,
            },
            &mut self.switcher,
        );
        self.draws.clear();
        self.session.app().draw(&mut self.draws);
        compositor.draw_list(&self.draws);
    }

    pub fn drive_emulator(&mut self) {
        self.session.set_driven(true);
    }

    pub fn step_emulator(&self, present: Duration, timeout: Duration) -> bool {
        self.session.step_emulator(present, timeout)
    }

    pub fn rebuilding_labels(&self) -> bool {
        self.rebuild.is_some()
    }

    fn draw_rebuild(&mut self, compositor: &mut Compositor) -> bool {
        let Some(rebuild) = &self.rebuild else {
            return false;
        };
        if rebuild.finished() {
            self.rebuild = None;
            self.last = Instant::now();
            return false;
        }
        let faces = &mut self.rebuild_faces;
        let title = *faces.title.get_or_insert_with(|| {
            let f = rebuild_title_face("Caching New Labels");
            (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
        });
        let done = rebuild.done();
        if faces.shown != Some(done) {
            let f = rebuild_count_face(&format!("{done} / {}", rebuild.total()));
            faces.count_size = (f.w, f.h);
            upload(compositor, &mut faces.count, f);
            faces.shown = Some(done);
        }
        let screen = RebuildScreen {
            title: Some(title),
            count: faces
                .count
                .map(|tex| (tex, faces.count_size.0, faces.count_size.1)),
            fraction: done as f32 / rebuild.total().max(1) as f32,
        };
        self.draws.clear();
        screen.draw(&mut self.draws);
        compositor.draw_list(&self.draws);
        true
    }

    pub fn advance(&mut self, input: &mut dyn InputSource) {
        let now = self.now();
        let mut events = input.poll(now);
        if !events.is_empty() {
            self.last_input=Instant::now();
            #[cfg(feature = "brick")]
            {self.woke_untouched=false;}
        }
        if self.wake_guard.is_some_and(|until| Instant::now() < until) {
            events.retain(|e| !matches!(e, slot_input::RawEvent::Down(slot_input::Btn::Power) | slot_input::RawEvent::Up(slot_input::Btn::Power)));
        }
        if self.rebuild.is_some() {
            return;
        }
        self.session.feed(events, now);
        let dt = self.last.elapsed().as_secs_f32();
        self.last = Instant::now();
        self.session.update(dt);
    }

    #[cfg(feature = "brick")]
    pub fn prepare_sleep(&mut self) -> bool {
        self.session.app_mut().settle_saves();
        self.session.observed_speed().is_none_or(|s| s == crate::emu::Speed::Paused)
    }

    #[cfg(feature = "brick")]
    pub fn resume_sleep(&mut self, input: &mut dyn InputSource) {
        let _ = input.poll(self.now()); // discard the kernel's queued wake key
        self.session.reset_sleep_input();
        self.session.app_mut().wake();
        self.last = Instant::now();
        self.wake_guard = Some(self.last + Duration::from_millis(750));
        self.last_input = self.last;
        self.woke_untouched = true;
        self.session.update(0.0);
    }

    /// Doze after a stretch without input; device_app then suspends as for a power press.
    #[cfg(feature = "brick")]
    pub fn idle_sleep(&mut self) {
        if !self.auto_sleep || self.rebuild.is_some() || self.app().link_active() {return}
        let limit = match self.app().phase() {
            Phase::Shelf | Phase::QuickMenu { .. } => IDLE_MENU,
            Phase::Playing { .. } => IDLE_GAME,
            _ => return,
        };
        let limit = if self.woke_untouched {limit.min(IDLE_AFTER_WAKE)} else {limit};
        if self.last_input.elapsed() >= limit {
            eprintln!("slot: idle for {}s; sleeping", self.last_input.elapsed().as_secs());
            self.woke_untouched = false;
            self.session.app_mut().idle_doze();
        }
    }

    pub fn app(&self) -> &crate::app::App {
        self.session.app()
    }

    pub fn core_settling(&self) -> bool {
        self.session.core_settling()
    }

    pub fn advance_at(&mut self, input: &mut dyn InputSource, now: Millis, dt: f32) {
        let events = input.poll(now);
        self.session.feed(events, now);
        self.session.update(dt);
    }

    pub fn now(&self) -> Millis {
        self.start.elapsed().as_millis() as Millis
    }

    pub fn powering_off(&self) -> bool {
        self.session.app().ready_to_power_off()
    }

    pub fn restarting(&self) -> bool {
        self.session.app().ready_to_restart()
    }

    pub fn restart(&mut self) {
        self.session.app_mut().restart();
    }

    #[cfg(feature = "brick")]
    pub fn exit_to_launcher(&mut self) {
        self.session.app_mut().settle_saves();
        self.session.app().restore_brick_leds();
        let _ = std::process::Command::new("sync").status();
        std::process::exit(0);
    }

    pub fn poweroff(&mut self) {
        self.session.app_mut().poweroff();
    }
}

fn menu_faces<'a>(
    compositor: &mut Compositor,
    labels: impl Iterator<Item = &'a str>,
) -> Vec<(TexId, u32, u32)> {
    labels
        .map(|label| {
            let f = menu_face(label);
            (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
        })
        .collect()
}

fn legend_faces(compositor: &mut Compositor, legend: &[(&str, &str)]) -> Vec<TexId> {
    legend
        .iter()
        .map(|(key, label)| {
            let f = hint_face(key, label);
            compositor.create_texture(f.w, f.h, &f.rgba)
        })
        .collect()
}

struct Faces<'a> {
    pool: &'a mut Vec<TexId>,
    title: &'a mut Option<TexId>,
    undo: &'a mut Option<TexId>,
}

fn sync_switcher(app: &mut App, compositor: &mut Compositor, texes: Faces, state: &mut Switcher) {
    if !matches!(app.phase(), Phase::Polaroids { .. }) {
        state.open = false;
        return;
    }
    if !state.open {
        state.open = true;
        state.titled = None;
        let faces: Vec<_> = app.polaroid_entries().iter().map(photo_face).collect();
        let ids = faces
            .iter()
            .enumerate()
            .map(|(i, f)| match texes.pool.get(i) {
                Some(id) => {
                    compositor.update_texture(*id, f.w, f.h, &f.rgba);
                    *id
                }
                None => {
                    let id = compositor.create_texture_nearest(f.w, f.h, &f.rgba);
                    texes.pool.push(id);
                    id
                }
            })
            .collect();
        app.set_polaroid_faces(ids);

        let label = app
            .undo_label()
            .map(|l| upload(compositor, texes.undo, hint_face("X", l)));
        app.set_undo_face(label);
    }
    if state.titled.as_deref() != app.polaroid_stamp() {
        state.titled = app.polaroid_stamp().map(str::to_string);
        let face = title_face(&app.polaroid_title(&format_stamp(app.wall_secs())));
        let id = upload(compositor, texes.title, face);
        app.set_polaroid_title_face(id);
    }
}

fn sync_clock(app: &mut App, compositor: &mut Compositor, clocks: &mut Clocks) {
    let picked = app.picker().map(|p| p.text());
    if picked != clocks.picked {
        clocks.picked = picked;
        if let (Some(face), Some(hint)) = (app.picker().map(|p| p.face()), clocks.hint) {
            let line = upload(compositor, &mut clocks.line, face);
            app.set_clock_faces(line, hint);
        }
    }
    let shown = hhmm(app.wall_secs());
    if shown != clocks.shown {
        let face = word_face(&shown);
        clocks.shown = shown;
        let w = face.w;
        let id = upload(compositor, &mut clocks.shelf, face);
        app.set_shelf_clock_face(id, w);
    }
    let platform_shown = app.slot_text().unwrap_or_default();
    if platform_shown != clocks.platform {
        clocks.platform = platform_shown.clone();
        if platform_shown.is_empty() {
            app.clear_shelf_platform();
        } else {
            let face = word_face(&platform_shown);
            let w = face.w;
            let id = upload(compositor, &mut clocks.platform_tex, face);
            app.set_shelf_platform_face(id, w);
        }
    }
    let battery_shown = app
        .battery()
        .map(|b| format!("{}%", b.percent))
        .unwrap_or_default();
    if battery_shown != clocks.battery {
        clocks.battery = battery_shown.clone();
        if !battery_shown.is_empty() {
            let face = word_face(&battery_shown);
            let w = face.w;
            let id = upload(compositor, &mut clocks.battery_tex, face);
            app.set_battery_percent_face(id, w);
        }
    }
}

fn sync_quick_clock(app: &mut App, compositor: &mut Compositor, state: &mut QuickClock) {
    if app.quick_menu().is_none() {
        return;
    }
    let text = date_time_text(app.wall_secs());
    if text == state.shown {
        return;
    }
    let (dim, lit) = (
        quick_value_face(&text, false),
        quick_value_face(&text, true),
    );
    let (dim_size, lit_size) = ((dim.w, dim.h), (lit.w, lit.h));
    let dim = upload(compositor, &mut state.dim, dim);
    let lit = upload(compositor, &mut state.lit, lit);
    app.set_quick_clock_faces((dim, dim_size.0, dim_size.1), (lit, lit_size.0, lit_size.1));
    state.shown = text;
}

fn sync_about(app: &mut App, compositor: &mut Compositor, state: &mut AboutFace) {
    if !matches!(app.phase(), Phase::About) {
        return;
    }
    let battery = app.battery().map(|b| b.percent);
    if state.tex.is_some() && state.battery == battery {
        return;
    }
    state.battery = battery;
    let build = Build::current();
    let face = sticker_face(&StickerFields {
        battery,
        serial: &build.serial(),
        dirty_digit: build.dirty_digit(),
    });
    let id = upload(compositor, &mut state.tex, face);
    app.set_sticker_face(id);
}

#[derive(Default)]
struct LabelAdjustTex {
    /// What was last drawn, so a frame that has not moved does no work at all.
    shown: Option<crate::app::AdjustKey>,
    picture: Option<TexId>,
    cart: Option<TexId>,
    caption: Option<TexId>,
    legend: [Option<TexId>; 5],
}

/// The picture being framed by hand, and the cart it is being framed for. Both are read and
/// uploaded once, when the framing screen opens; after that only the words change.
fn sync_label_adjust(app: &mut App, compositor: &mut Compositor, tex: &mut LabelAdjustTex) {
    let key = app.label_adjust_key();
    if key.is_none() || (tex.shown == key && app.label_adjust_built().is_some()) {
        return;
    }
    tex.shown = key;
    let Some((source, words, cart)) = app.label_adjust_view() else {
        return;
    };
    if app.label_adjust_built() == Some((source.as_path(), words.as_str())) {
        return;
    }
    let same_picture = app.label_adjust_built().is_some_and(|(built, _)| built == source);
    let caption = face(compositor, &mut tex.caption, slot_ui::caption_face(&words));
    if same_picture {
        // Only the frame moved: the picture and the cart are the ones already uploaded.
        if let (Some(picture), Some((legend, cart))) = (tex.picture, app.label_adjust_legend()) {
            app.set_label_adjust_faces((source, words), picture, cart, caption, legend);
        }
        return;
    }
    let Some((rgba, w, h)) =
        std::fs::read(&source).ok().and_then(|png| crate::cart_art::label_picture(&png, ADJUST_TEX_MAX))
    else {
        return;
    };
    let picture = upload_rgba(compositor, &mut tex.picture, w, h, &rgba);
    let (face_rgba, _) = slot_ui::cart_cutout_face(&cart);
    let cart_tex = upload_rgba(compositor, &mut tex.cart, face_rgba.w, face_rgba.h, &face_rgba.rgba);
    let mut legend = [(picture, 0u32); 5];
    for (i, hint) in slot_ui::label_adjust_legend_faces().into_iter().enumerate() {
        let (id, lw, _) = face(compositor, &mut tex.legend[i], hint);
        legend[i] = (id, lw);
    }
    app.set_label_adjust_faces((source, words), picture, cart_tex, caption, legend);
}

/// The sticker's curls and its shadow: the same textures every time, written over.
#[derive(Default)]
struct PeelTex {
    curls: Vec<Option<TexId>>,
    shadow: Option<TexId>,
}

/// A label just kept: its curls, baked off the UI thread, go up once, which sets it off.
fn sync_peel(app: &mut App, compositor: &mut Compositor, tex: &mut PeelTex) {
    let Some((curls, shadow)) = app.peel_unbaked() else {
        return;
    };
    tex.curls.resize(curls.len().max(tex.curls.len()), None);
    let ids = curls.iter().zip(&mut tex.curls).map(|(c, at)| upload_rgba(compositor, at, c.w, c.h, &c.rgba)).collect();
    let shadow = upload_rgba(compositor, &mut tex.shadow, shadow.w, shadow.h, &shadow.rgba);
    app.set_peel_faces(ids, shadow);
}

/// How large the picture being framed is kept: enough to zoom into, and no larger.
const ADJUST_TEX_MAX: u32 = 1024;

#[derive(Default)]
struct LabelGridTex {
    shown: Option<crate::app::GridKey>,
    title: Option<TexId>,
    caption: Option<TexId>,
    legend: [Option<TexId>; 3],
    cells: Vec<Option<TexId>>,
}

/// The label grid's page: its labels as pictures, its heading and its words. The pictures are
/// read again only when the page turns or another label lands on it; the words, which change
/// as the cursor moves and the search runs, are drawn on their own.
fn sync_label_grid(app: &mut App, compositor: &mut Compositor, tex: &mut LabelGridTex) {
    // Worked out without allocating; what follows is not.
    let key = app.label_grid_key();
    if key.is_none() || (tex.shown == key && app.label_grid_built().is_some()) {
        return;
    }
    tex.shown = key;
    let Some((page, paths, words)) = app.label_grid_page() else {
        return;
    };
    let fresh = app.label_grid_built() == Some((page, paths.len()));
    if fresh {
        if app.label_grid_caption_built() != Some(words.as_str()) {
            let caption = face(compositor, &mut tex.caption, slot_ui::caption_face(&words));
            app.set_label_grid_caption(words, caption);
        }
        return;
    }
    let query = app.label_grid_query().unwrap_or_default();
    let heading =
        if query.is_empty() { "FIND A LABEL".to_string() } else { format!("FIND A LABEL \u{b7} {query}") };
    let title = face(compositor, &mut tex.title, slot_ui::title_face(&heading));
    let caption = face(compositor, &mut tex.caption, slot_ui::caption_face(&words));
    let mut legend = [(caption.0, 0u32); 3];
    for (i, hint) in slot_ui::label_grid_legend_faces().into_iter().enumerate() {
        let (id, w, _) = face(compositor, &mut tex.legend[i], hint);
        legend[i] = (id, w);
    }
    tex.cells.resize(slot_ui::PAGE, None);
    let cells = (0..slot_ui::PAGE)
        .map(|i| {
            let png = std::fs::read(paths.get(i)?).ok()?;
            let (rgba, w, h) = crate::cart_art::label_picture(&png, GRID_TEX_MAX)?;
            Some((upload_rgba(compositor, &mut tex.cells[i], w, h, &rgba), w, h))
        })
        .collect();
    app.set_label_grid_faces((page, paths.len()), words, title, caption, cells, legend);
}

/// Uploads a face and keeps its size, which is what the views are drawn from.
fn face(compositor: &mut Compositor, slot: &mut Option<TexId>, f: slot_ui::UndoFace) -> (TexId, u32, u32) {
    let (w, h) = (f.w.max(1), f.h.max(1));
    // No font: a clear pixel, so there is still a texture to point at.
    let rgba = if f.rgba.is_empty() { vec![0; 4] } else { f.rgba };
    let (w, h) = if rgba.len() as u32 == w * h * 4 { (w, h) } else { (1, 1) };
    (upload_rgba(compositor, slot, w, h, &rgba), w, h)
}

/// How big a label is kept for the grid: four to a row on a 640 wide screen.
const GRID_TEX_MAX: u32 = 160;

fn sync_core_picker(
    app: &mut App,
    compositor: &mut Compositor,
    builder: &FaceBuilder,
    asked: &mut Option<String>,
    board: &mut Option<TexId>,
    lid: &mut Option<TexId>,
    built: &mut Option<String>,
) {
    // Keyed by the cart and the plastic it wears, so a colour or finish changed in the picker
    // (or the cart menu) rebuilds the lid it shows.
    let cart = app.selected_stem().and_then(|stem| app.carts().find(|c| c.stem == stem)).cloned();
    let highlighted = cart.as_ref().map(crate::face_builder::face_key);
    if let Some(cart) = cart {
        if *asked != highlighted {
            builder.request(cart);
            *asked = highlighted.clone();
        }
    }
    let Some(faces) = builder.take() else {
        return;
    };
    if highlighted.as_deref() != Some(faces.key.as_str()) || *built == highlighted {
        return;
    }
    let board_id = upload_rgba(
        compositor,
        board,
        faces.board.w,
        faces.board.h,
        &faces.board.rgba,
    );
    let lid_id = upload_rgba(compositor, lid, faces.lid.w, faces.lid.h, &faces.lid.rgba);
    app.set_core_board_faces(board_id, lid_id);
    *built = Some(faces.key);
}

fn upload(compositor: &mut Compositor, slot: &mut Option<TexId>, face: slot_ui::UndoFace) -> TexId {
    upload_rgba(compositor, slot, face.w, face.h, &face.rgba)
}

fn upload_rgba(
    compositor: &mut Compositor,
    slot: &mut Option<TexId>,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> TexId {
    match *slot {
        Some(id) => {
            compositor.update_texture(id, w, h, rgba);
            id
        }
        None => {
            let id = compositor.create_texture(w, h, rgba);
            *slot = Some(id);
            id
        }
    }
}

#[cfg(feature = "brick")]
fn platform_root_has(root: &std::path::Path, name: &str) -> bool {
    root.join(name).exists()
}

#[cfg(all(test,feature="brick"))]
mod power_render_tests {
    use super::*;
    fn booted(dir:&std::path::Path)->Frontend {
        crate::root::ensure(dir);
        let mut state=slot_store::SlotState::default();state.clock_set=true;
        slot_store::write_slot_state(dir,&state).unwrap();
        Frontend::boot(Box::new(slot_power::SimPlatform::at(dir.into())))
    }
    #[test]
    fn idle_shelf_dozes_after_limit_only() {
        let dir=tempfile::tempdir().unwrap();let mut f=booted(dir.path());
        assert!(matches!(f.app().phase(),Phase::Shelf));
        f.last_input=Instant::now()-Duration::from_secs(4*60);f.idle_sleep();
        assert!(matches!(f.app().phase(),Phase::Shelf),"4 minutes is not yet idle");
        f.last_input=Instant::now()-Duration::from_secs(5*60+1);f.idle_sleep();
        assert!(matches!(f.app().phase(),Phase::Doze{..}));
    }
    #[test]
    fn untouched_wake_sleeps_again_quickly() {
        let dir=tempfile::tempdir().unwrap();let mut f=booted(dir.path());
        f.woke_untouched=true;
        f.last_input=Instant::now()-Duration::from_secs(60);f.idle_sleep();
        assert!(matches!(f.app().phase(),Phase::Shelf));
        f.last_input=Instant::now()-Duration::from_secs(91);f.idle_sleep();
        assert!(matches!(f.app().phase(),Phase::Doze{..}));
    }
    #[test]
    fn auto_sleep_can_be_disabled() {
        let dir=tempfile::tempdir().unwrap();crate::root::ensure(dir.path());
        std::fs::write(dir.path().join("DISABLE_AUTO_SLEEP"),"").unwrap();
        let mut f=booted(dir.path());
        f.last_input=Instant::now()-Duration::from_secs(3600);f.idle_sleep();
        assert!(matches!(f.app().phase(),Phase::Shelf));
    }
    #[test]
    fn idle_redraw_gate_preserves_input_effects_and_periodic_refresh() {
        let dir=tempfile::tempdir().unwrap();
        crate::root::ensure(dir.path());
        let mut state=slot_store::SlotState::default();state.clock_set=true;
        slot_store::write_slot_state(dir.path(),&state).unwrap();
        let mut f=Frontend::boot(Box::new(slot_power::SimPlatform::at(dir.path().into())));
        assert!(matches!(f.app().phase(),Phase::Shelf));
        f.last_input=Instant::now()-Duration::from_secs(3);
        f.last_render=Instant::now();f.rendered_effects=f.render_effects();
        let mut scene=vec![];f.app().draw(&mut scene);f.draws=scene;
        assert!(!f.needs_render(),"unchanged shelf should not redraw");
        f.last_input=Instant::now();assert!(f.needs_render(),"input must redraw immediately");
        f.last_input=Instant::now()-Duration::from_secs(3);
        f.rendered_effects.0=-1.0;assert!(f.needs_render(),"screen transitions must animate");
        f.rendered_effects=f.render_effects();f.last_render=Instant::now()-Duration::from_secs(2);
        assert!(f.needs_render(),"clock and asynchronous artwork must refresh");
        f.last_render=Instant::now();f.draws.clear();
        assert!(f.needs_render(),"changed drawing commands must redraw");
    }
}
