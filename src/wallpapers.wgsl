const WP_TEAL = vec3(0.0, 245.0, 212.0) / 255.0;
const WP_TEXT = vec3(248.0, 250.0, 252.0) / 255.0;
const WP_MUTED = vec3(148.0, 163.0, 184.0) / 255.0;
const WP_PURPLE = vec3(192.0, 132.0, 252.0) / 255.0;
const WP_PANEL = vec3(14.0, 9.0, 28.0) / 255.0;
const WP_BORDER = vec3(47.0, 28.0, 86.0) / 255.0;
const WP_TAB_LEFT = array(20.0, 221.0, 401.0, 525.0);
const WP_TAB_LABEL = array(WP_DOWNLOADED, WP_STEAM, WP_MOTION, WP_PLAYLIST);
@group(0) @binding(6) var wall_posters: texture_2d_array<f32>;
@group(0) @binding(12) var wall_preview: texture_2d<f32>;

const W_TODOS = array(1868853076u, 115u, 0u, 0u, 0u, 0u);
const W_WORKSHOP = array(1802661719u, 1886349427u, 0u, 0u, 0u, 0u);
const W_MOTION = array(1769238349u, 1195535983u, 115u, 0u, 0u, 0u);
const W_IMPORTADOS = array(1869638985u, 1684108402u, 29551u, 0u, 0u, 0u);
const W_MINECRAFT = array(1701734733u, 1717662307u, 116u, 0u, 0u, 0u);
const W_ANIME = array(1835626049u, 101u, 0u, 0u, 0u, 0u);
const W_NATURALEZA = array(1970561358u, 1701601650u, 24954u, 0u, 0u, 0u);
const W_CYBERPUNK = array(1700952387u, 1853190258u, 107u, 0u, 0u, 0u);
const W_ESPACIO = array(1634759493u, 7301475u, 0u, 0u, 0u, 0u);
const W_PIXEL = array(1702390096u, 1916870764u, 116u, 0u, 0u, 0u);
const W_FANTASIA = array(1953390918u, 1642951521u, 0u, 0u, 0u, 0u);
const W_JUEGOS = array(1734702410u, 29551u, 0u, 0u, 0u, 0u);
const W_RELAX = array(1768320844u, 1377840928u, 2019650661u, 0u, 0u, 0u);
const W_ATARDECER = array(1918989377u, 1701012836u, 114u, 0u, 0u, 0u);
const W_PAISAJES = array(1936286032u, 1936026209u, 0u, 0u, 0u, 0u);
const W_SCIFI = array(761881427u, 26950u, 0u, 0u, 0u, 0u);
const W_BUSCAR = array(1668511042u, 29281u, 0u, 0u, 0u, 0u);
const W_AGREGAR = array(1732321323u, 1634166130u, 114u, 0u, 0u, 0u);
const W_CARPETA = array(1886544195u, 6386789u, 0u, 0u, 0u, 0u);
const W_REFRESCAR = array(1970561857u, 2053729377u, 29281u, 0u, 0u, 0u);
const W_PAG = array(1768415568u, 24942u, 0u, 0u, 0u, 0u);
const W_WORKSHOP_TAG = array(1634038867u, 109u, 0u, 0u, 0u, 0u);
const W_DESCARGAR = array(1668506948u, 1634169441u, 114u, 0u, 0u, 0u);
const W_DESCARGAR4K = array(1668506948u, 1634169441u, 1261707378u, 0u, 0u, 0u);
const W_IMAGEN = array(1734438217u, 28261u, 0u, 0u, 0u, 0u);
const W_WEB = array(543319383u, 1648435383u, 7498098u, 0u, 0u, 0u);
const W_PRESET = array(1936028240u, 3072357477u, 1919041824u, 29289u, 0u, 0u);
const W_INSTALADO = array(1953721929u, 1684106337u, 111u, 0u, 0u, 0u);
const W_MOTSEARCH = array(1668511042u, 1696625249u, 1867325550u, 1852795252u, 7554882u, 0u);
const W_NATIVEMSG = array(544106835u, 1919968626u, 1668637807u, 1861445987u, 1952542240u, 6387305u);
const W_PROPIEDADES = array(1886351952u, 1633969513u, 544433508u, 543974756u, 1684959078u, 111u);
const W_RESTABLECER = array(1953719634u, 1634891105u, 114u, 0u, 0u, 0u);
const W_ANTERIOR = array(60u, 0u, 0u, 0u, 0u, 0u);
const W_SIGUIENTE = array(62u, 0u, 0u, 0u, 0u, 0u);
const W_MAS = array(43u, 0u, 0u, 0u, 0u, 0u);
const W_MENOS = array(45u, 0u, 0u, 0u, 0u, 0u);
const W_STEAM_FILTERS = array(W_TODOS, W_MINECRAFT, W_ANIME, W_PAISAJES, W_PIXEL, W_CYBERPUNK, W_RELAX, W_FANTASIA, W_SCIFI, W_JUEGOS);
const W_MOTION_FILTERS = array(W_MINECRAFT, W_ANIME, W_NATURALEZA, W_CYBERPUNK, W_ESPACIO, W_PIXEL, W_FANTASIA, W_JUEGOS, W_RELAX, W_ATARDECER);
const W_ENLACE = array(1634496069u, 25955u, 0u, 0u, 0u, 0u);
const W_APLICAR = array(1768714305u, 7496035u, 0u, 0u, 0u, 0u);
const W_DIRECTO = array(1886680168u, 791624307u, 1831743022u, 13424u, 0u, 0u);
const W_WSEARCH = array(1668511042u, 1696625249u, 1867980910u, 1752394610u, 28783u, 0u);

fn wp_shape(ui: vec4<f32>, q: vec2<f32>, size: vec2<f32>, radius: f32, fill: vec3<f32>,
            border: vec3<f32>, dpi: f32) -> vec4<f32> {
    let d = rounded_distance(q, size, radius);
    let a = coverage(d, dpi);
    if a <= 0.0 { return ui; }
    return over(over(ui, fill, a), border, a - coverage(d + 1.0, dpi));
}

fn wp_label(ui: vec4<f32>, pt: vec2<f32>, left: f32, mid_y: f32, s: DockSprite, tint: vec3<f32>) -> vec4<f32> {
    return over(ui, tint, dock_sprite(s, pt - vec2(left, mid_y - s.rect.w * 0.5) + s.origin).a);
}

fn wp_glyph(code: u32, pt: vec2<f32>) -> f32 {
    if code < 32u || code > 255u || any(pt < vec2(0.0)) || pt.x >= 12.0 || pt.y >= 18.0 { return 0.0; }
    let i = code - 32u;
    let q = vec2(f32(i % 16u) * 12.0, f32(i / 16u) * 18.0) + pt;
    return dock_sprite(WP_GLYPHS, q).a;
}

fn wp_caption(ui: vec4<f32>, pt: vec2<f32>, words: array<u32, 6>, tint: vec3<f32>) -> vec4<f32> {
    if pt.x < 0.0 || pt.x >= 192.0 || abs(pt.y) >= 9.0 { return ui; }
    let k = u32(pt.x / 8.0);
    return over(ui, tint, wp_glyph((words[k / 4u] >> ((k % 4u) * 8u)) & 255u, vec2(pt.x - f32(k) * 8.0, pt.y + 9.0)));
}

fn wp_pill(ui: vec4<f32>, pt: vec2<f32>, rect: vec4<f32>, id: u32, selected: bool,
           words: array<u32, 6>, dpi: f32) -> vec4<f32> {
    let q = pt - rect.xy;
    if any(q < vec2(-1.0)) || any(q > rect.zw + 1.0) { return ui; }
    let hot = p.wall_ui.y == id;
    let tint = select(select(WP_MUTED, WP_TEXT, hot), WP_TEAL, selected);
    let bg = wp_shape(ui, q, rect.zw, 8.0, mix(WP_PANEL, tint, select(0.05, 0.16, hot || selected)),
                      mix(WP_BORDER, tint, select(0.10, 0.65, hot || selected)), dpi);
    return wp_caption(bg, q - vec2(12.0, rect.w * 0.5), words, tint);
}

fn wp_advance(text: u32, i: u32) -> u32 {
    return (p.wall_advances[text * 2u + i / 16u][(i / 4u) % 4u] >> ((i % 4u) * 8u)) & 255u;
}

fn wp_line(ui: vec4<f32>, pt: vec2<f32>, text: u32, tint: vec3<f32>) -> vec4<f32> {
    let metrics = p.wall_rows[text].z;
    if pt.x < 0.0 || pt.x >= f32(metrics >> 8u) || abs(pt.y) >= 9.0 { return ui; }
    var lo = 0u;
    var hi = metrics & 255u;
    while hi > lo + 1u {
        let mid = (lo + hi) / 2u;
        if f32(wp_advance(text, mid)) > pt.x { hi = mid; } else { lo = mid; }
    }
    let code = (p.wall_words[text * 2u + lo / 16u][(lo / 4u) % 4u] >> ((lo % 4u) * 8u)) & 255u;
    return over(ui, tint, wp_glyph(code, vec2(pt.x - f32(wp_advance(text, lo)) + 1.0, pt.y + 9.0)));
}

fn wp_number(ui: vec4<f32>, pt: vec2<f32>, value: u32, tint: vec3<f32>) -> vec4<f32> {
    let n = min(value, 9999u);
    let digits = 1u + u32(n >= 10u) + u32(n >= 100u) + u32(n >= 1000u);
    let x = pt.x + f32(digits) * 4.0;
    if x < 0.0 || x >= f32(digits) * 8.0 || abs(pt.y) > 9.0 { return ui; }
    let zero = u32(x / 8.0);
    let divisor = array(1u, 10u, 100u, 1000u);
    let digit = n / divisor[digits - zero - 1u] % 10u;
    return over(ui, tint, wp_glyph(48u + digit, vec2(x - f32(zero) * 8.0, pt.y + 8.0)));
}

fn wp_button(ui: vec4<f32>, pt: vec2<f32>, rect: vec4<f32>, id: u32,
             label: DockSprite, tint: vec3<f32>, dpi: f32) -> vec4<f32> {
    if any(pt < rect.xy - 1.0) || any(pt > rect.xy + rect.zw + 1.0) { return ui; }
    let hot = p.wall_ui.y == id;
    let box = wp_shape(ui, pt - rect.xy, rect.zw, 8.0, mix(WP_PANEL, tint, select(0.03, 0.19, hot)),
                       mix(WP_BORDER, tint, select(0.12, 0.58, hot)), dpi);
    return wp_label(box, pt, rect.x + (rect.z - label.rect.z) * 0.5, rect.y + rect.w * 0.5, label, tint);
}

fn wp_header(below: vec4<f32>, pt: vec2<f32>, dpi: f32) -> vec4<f32> {
    let q = pt - vec2(20.0, 14.0);
    let hot = p.wall_ui.y;
    var ui = wp_shape(below, q, vec2(34.0), 8.0, mix(WP_PANEL, WP_PURPLE, select(0.12, 0.24, hot == 2u)), WP_BORDER, dpi);
    ui = over(ui, WP_PURPLE, dock_icon(6u, DOCK_BUTTONS[6].xy + DOCK_BUTTONS[6].zw * 0.5 + (q - 17.0) * 1.25).a * coverage(length(q - 17.0) - 15.0, dpi));
    let pill = pt - vec2(66.0, 16.0);
    let s = vec2(129.0, 30.0);
    let edge = rounded_distance(pill, s, 8.0);
    ui = over(ui, vec3(0.025, 0.10, 0.14), coverage(edge, dpi));
    if abs(edge) < 6.0 {
        let phase = fract((pill.x + p.dock_play.w) / 260.0);
        let cyan = mix(vec3(0.025, 0.40, 0.45), vec3(0.49, 0.99, 0.97), 1.0 - abs(phase * 2.0 - 1.0));
        ui = hairline(ui, edge, cyan, vec2(0.55, 1.0), 0.38, dpi);
    }
    ui = wp_label(ui, pt, 66.0 + (129.0 - WP_TITLE.rect.z) * 0.5, 31.0, WP_TITLE, WP_TEAL);
    let sound = p.wall_rows[73];
    if sound.z > 0u {
        let at = pt - vec2(1017.0, 14.0);
        let playing = sound.x != 0u;
        let tint = select(WP_MUTED, WP_TEAL, playing || hot == 29u);
        ui = wp_shape(ui, at, vec2(173.0, 34.0), 8.0, WP_PANEL,
                      mix(WP_BORDER, tint, select(0.2, 0.7, hot == 29u)), dpi);
        let speaker = at - vec2(18.0, 17.0);
        if all(abs(speaker) < vec2(12.0)) {
            let d = min(segment_distance(speaker, vec2(-6.0, 0.0), vec2(-4.0, 0.0)) - 1.9,
                        segment_distance(speaker, vec2(-3.0, 0.0), vec2(0.0, 0.0)) - 3.0);
            let wave = abs(length(speaker - vec2(0.0, 0.0)) - 6.3) - 0.85;
            let cross = min(segment_distance(speaker, vec2(4.4, -2.5), vec2(9.1, 2.5)),
                            segment_distance(speaker, vec2(4.4, 2.5), vec2(9.1, -2.5))) - 0.80;
            ui = over(ui, tint, coverage(min(d, select(cross, max(wave, 3.0 - speaker.x), playing)), dpi));
        }
        ui = wp_line(ui, at - vec2(37.0, 17.0), 73u, tint);
    }
    let close = pt - vec2(1204.0, 14.0);
    ui = over(ui, vec3(0.91, 0.07, 0.14), f32(hot == 1u) * coverage(rounded_distance(close, vec2(37.0, 34.0), 8.0), dpi));
    return over(ui, mix(WP_MUTED, WP_TEXT, f32(hot == 1u)), centered_mask(CONTROL_SPRITES[2], close - vec2(18.5, 17.0)));
}

fn wp_tab_icon(i: u32, q: vec2<f32>, dpi: f32) -> f32 {
    if any(abs(q) > vec2(9.0)) { return 0.0; }
    var d = 30.0;
    if i == 0u {
        d = min(segment_distance(q, vec2(0.0,-6.0), vec2(0.0,3.0)),
            min(segment_distance(q, vec2(-4.0,-1.0), vec2(0.0,3.0)), segment_distance(q, vec2(0.0,3.0), vec2(4.0,-1.0))));
        d = min(d, min(segment_distance(q, vec2(-7.0,6.0), vec2(7.0,6.0)),
            min(segment_distance(q, vec2(-7.0,2.0), vec2(-7.0,6.0)), segment_distance(q, vec2(7.0,2.0), vec2(7.0,6.0)))));
    } else if i == 1u {
        d = min(abs(length(q - vec2(2.0,-2.0)) - 4.1), abs(length(q + vec2(4.0,-4.0)) - 2.3));
        d = min(d, segment_distance(q, vec2(-3.0,4.0), vec2(0.0,1.0)));
    } else if i == 2u {
        d = min(abs(rounded_distance(q + vec2(7.0), vec2(14.0), 2.0)),
            min(length(q + vec2(3.0)), segment_distance(q, vec2(-6.0,4.0), vec2(2.0,-1.0))));
    } else {
        d = min(segment_distance(q, vec2(-5.0,-6.0), vec2(-5.0,6.0)),
            min(segment_distance(q, vec2(-5.0,-6.0), vec2(5.0,0.0)), segment_distance(q, vec2(-5.0,6.0), vec2(5.0,0.0))));
    }
    return coverage(d - 0.85, dpi);
}

fn wp_tabs(below: vec4<f32>, pt: vec2<f32>, dpi: f32) -> vec4<f32> {
    var ui = below;
    for (var i = 0u; i < 4u; i++) {
        let x = WP_TAB_LEFT[i] + 14.0;
        let label = WP_TAB_LABEL[i];
        let tint = select(select(WP_MUTED, WP_TEXT, p.wall_ui.y == 4u + i), WP_TEAL, p.wall_ui.x == i);
        ui = wp_label(ui, pt, x + 24.0, 82.0, label, tint);
        ui = over(ui, tint, wp_tab_icon(i, pt - vec2(x + 8.0, 82.0), dpi));
        if i == 0u || i == 3u {
            let cx = x + 46.0 + label.rect.z;
            ui = wp_shape(ui, pt - vec2(cx - 13.0, 73.0), vec2(26.0, 18.0), 8.0, WP_TEAL * 0.19, WP_TEAL * 0.42, dpi);
            ui = wp_number(ui, pt - vec2(cx, 82.0), u32(select(p.wall_state.y, p.wall_state.x, i == 0u)), WP_TEAL);
        }
    }
    return over(ui, WP_TEAL, coverage(rounded_distance(pt - vec2(p.wall_grid.z, 100.5), vec2(p.wall_grid.w, 2.5), 1.25), dpi));
}

fn wp_toolbar(below: vec4<f32>, pt: vec2<f32>, dpi: f32) -> vec4<f32> {
    let tab = p.wall_ui.x;
    if tab == 0u { return wp_downloaded_toolbar(below, pt, dpi); }
    var ui = below;
    if tab < 3u {
        let online = (p.wall_ui.z & 16u) != 0u;
        let w = select(504.0, 1108.0, tab == 2u);
        let edit = (p.wall_ui.z & 1u) != 0u;
        ui = wp_shape(ui, pt - vec2(20.0, 114.0), vec2(w, 36.0), 8.0, WP_PANEL,
                      select(WP_BORDER, WP_TEAL * 0.70, edit), dpi);
        let lens = pt - vec2(38.0, 131.0);
        let d = min(abs(length(lens + 1.0) - 4.5), segment_distance(lens, vec2(2.2), vec2(6.4)));
        ui = over(ui, WP_MUTED, coverage(d - 0.8, dpi));
        if p.wall_rows[31].z > 0u {
            ui = wp_line(ui, pt - vec2(55.0, 131.0), 31u, WP_TEXT);
        } else if !edit {
            if tab == 1u && online { ui = wp_caption(ui, pt - vec2(55.0, 132.0), W_WSEARCH, WP_MUTED); }
            else if tab == 1u { ui = wp_label(ui, pt, 55.0, 132.0, WP_SEARCH_STEAM, WP_MUTED); }
            else { ui = wp_caption(ui, pt - vec2(55.0, 132.0), W_MOTSEARCH, WP_MUTED); }
        }
        if edit {
            let x = 56.0 + f32(p.wall_rows[31].z >> 8u);
            ui = over(ui, WP_TEXT, coverage(abs(pt.x - x) - 0.5, dpi) * coverage(abs(pt.y - 132.0) - 7.0, dpi));
        }
        if tab == 1u {
            ui = wp_button(ui, pt, vec4(646.0, 114.0, 141.0, 36.0), 14u, WP_LOCAL, select(WP_TEAL, WP_MUTED, online), dpi);
            ui = wp_button(ui, pt, vec4(797.0, 114.0, 243.0, 36.0), 13u, WP_ONLINE, select(WP_MUTED, WP_TEAL, online), dpi);
            if online {
                ui = wp_pill(ui, pt, vec4(534.0, 114.0, 100.0, 36.0), 15u, true, W_BUSCAR, dpi);
                ui = wp_shape(ui, pt - vec2(1050.0, 114.0), vec2(190.0, 36.0), 8.0, WP_PANEL,
                              select(WP_BORDER, WP_TEAL * 0.6, p.wall_ui.y == 16u), dpi);
                ui = wp_line(ui, pt - vec2(1062.0, 132.0), 33u, WP_TEXT);
                ui = over(ui, WP_MUTED, wp_glyph(118u, pt - vec2(1218.0, 123.0)));
            }
        } else if tab == 2u {
            ui = wp_pill(ui, pt, vec4(1138.0, 114.0, 102.0, 36.0), 15u, true, W_BUSCAR, dpi);
        }
        if pt.y >= 157.0 && pt.y <= 191.0 && pt.x >= 19.0 {
            let col = u32((pt.x - 19.0) / 116.0);
            if col < 10u {
                var words = W_MOTION_FILTERS[col];
                if tab == 1u { words = W_STEAM_FILTERS[col]; }
                ui = wp_pill(ui, pt, vec4(20.0 + f32(col) * 116.0, 158.0, 108.0, 32.0), 22u + col,
                             col == p.wall_catalog.x, words, dpi);
            }
        }
    } else if tab == 3u {
        let on = (p.wall_ui.z & 4u) != 0u;
        if on { ui = wp_button(ui, pt, vec4(20.0, 114.0, 198.0, 36.0), 9u, WP_ON, WP_TEAL, dpi); }
        else { ui = wp_button(ui, pt, vec4(20.0, 114.0, 198.0, 36.0), 9u, WP_OFF, WP_MUTED, dpi); }
        ui = wp_label(ui, pt, 238.0, 132.0, WP_DURATION, WP_MUTED);
        ui = wp_shape(ui, pt - vec2(302.0, 114.0), vec2(119.0, 36.0), 8.0, WP_PANEL, select(WP_BORDER, WP_TEAL * 0.70, p.wall_ui.y == 10u), dpi);
        ui = wp_number(ui, pt - vec2(356.0, 132.0), p.wall_ui.w, WP_TEXT);
        ui = over(ui, WP_MUTED, wp_glyph(115u, pt - vec2(381.0,123.0)));
        ui = wp_button(ui, pt, vec4(448.0, 114.0, 182.0, 36.0), 11u, WP_ADD, WP_TEAL, dpi);
        ui = wp_button(ui, pt, vec4(641.0, 114.0, 120.0, 36.0), 12u, WP_CLEAR, WP_MUTED, dpi);
    }
    return ui;
}

fn wp_title_line(ui: vec4<f32>, q: vec2<f32>, index: u32, y: f32) -> vec4<f32> {
    let left = 116.0 - f32(p.wall_rows[index].z >> 8u) * 0.5;
    let shade = wp_line(ui, q - vec2(left + 0.5, y + 1.25), index, vec3(0.025));
    return wp_line(shade, q - vec2(left, y), index, WP_TEXT);
}

fn wp_poster_title(ui: vec4<f32>, q: vec2<f32>, layer: i32) -> vec4<f32> {
    if q.x < 8.0 || q.x > 224.0 || q.y < 117.0 || q.y > 147.0 { return ui; }
    let extent = vec2(232.0, 180.0);
    let two = textureSampleLevel(wall_posters, smooth_sampler, vec2(2.5, 162.5) / extent, layer, 0.0).r > 0.5;
    let second = q.y >= 132.0;
    let center = select(132.0, select(124.5, 139.5, second), two);
    let dy = q.y - center;
    if abs(dy) >= select(9.0, 7.5, two) { return ui; }
    let y = select(170.0 + dy, select(165.0, 175.0, second) + clamp(dy * 0.625, -4.5, 4.5), two);
    let mask = textureSampleLevel(wall_posters, smooth_sampler, vec2(q.x, y) / extent, layer, 0.0).r;
    return over(ui, WP_TEXT, mask);
}

fn wp_card(ui: vec4<f32>, q: vec2<f32>, index: u32, dpi: f32) -> vec4<f32> {
    let item = p.wall_rows[index];
    let d = rounded_distance(q, vec2(232.0, 160.0), 10.0);
    let a = coverage(d, dpi);
    let hot = p.wall_ui.y == 32u + item.w;
    let is_current = (item.y & 1u) != 0u;
    let badge = (item.y & 2u) != 0u;
    let added = (item.y & 4u) != 0u;
    let playable = (item.y & 8u) != 0u;
    let shadow = rounded_distance(q - vec2(0.0, select(4.0, 8.0, hot)), vec2(232.0, 160.0), 10.0);
    var out = over(ui, vec3(0.0), select(0.40, 0.70, hot) * gaussian_cdf(-shadow / select(5.0, 8.0, hot)));
    if a <= 0.0 { return out; }
    var paint = vec3(0.029, 0.028, 0.10) + (1.0 - q.y / 160.0) * vec3(0.031, 0.02, 0.095);
    let size = vec2(232.0, 160.0);
    let aspect = 232.0 / 160.0;
    let zoom = 1.0 + 0.03 * f32(item.y >> 24u) / 255.0;
    let art = (q - size * 0.5) / zoom + size * 0.5;
    if (item.y & 131072u) != 0u {
        paint = textureSampleLevel(wall_preview, smooth_sampler, art / size, 0.0).rgb;
    } else if (item.y & 65536u) != 0u {
        let uv = 0.5 + (art / size - 0.5) * cover_scale(aspect, p.screen.z);
        if p.motion[7].w > 0.5 { paint = shared_cover_rgb(uv, 232.0 / 160.0); }
        else { paint = linear_to_srgb(sample_frame(uv, false, 0.0)); }
    } else if item.x >= 4u {
        let uv = clamp(art, vec2(0.25), size - 0.25) / vec2(232.0, 180.0);
        paint = textureSampleLevel(wall_posters, smooth_sampler, uv, i32(item.x - 4u), 0.0).rgb;
    } else if item.x > 0u {
        paint = dock_sprite(WP_POSTERS[item.x - 1u], art).rgb;
    }
    var card = vec4(paint, 1.0);
    if q.y >= 104.0 {
        card = over(card, vec3(8.0, 8.0, 28.0) / 255.0, 0.38);
        if item.x >= 4u && (item.y & 2048u) != 0u {
            card = wp_poster_title(card, q, i32(item.x - 4u));
        } else {
            let second = 43u + index;
            let two = p.wall_rows[second].z != 0u;
            card = wp_title_line(card, q, index, select(132.0, 124.5, two));
            if two { card = wp_title_line(card, q, second, 139.5); }
        }
    }
    if q.y < 36.0 {
        card = wp_catalog_badges(card, q, item.y, p.wall_sizes[index / 4u][index % 4u], dpi);
    }
    if q.y >= 76.0 && q.y < 102.0 && (item.y & 1024u) != 0u {
        card = wp_chip(card, q - vec2(8.0, 81.0), 88.0, WP_PANEL, 0.90, WP_TEAL, 0.40, dpi);
        card = wp_caption(card, q - vec2(16.0, 90.0), W_INSTALADO, WP_TEAL);
    }
    if p.wall_ui.x == 3u && added {
        let q2 = q - vec2(210.0, 90.0);
        card = wp_shape(card, q2 + 10.0, vec2(20.0), 5.0, WP_PANEL, WP_TEAL * 0.72, dpi);
        let d2 = max(abs(q2.y) - 0.7, abs(q2.x) - 4.5);
        card = over(card, WP_TEAL, coverage(d2, dpi));
    }
    if badge && playable && hot {
        let gear = q - vec2(183.0, 90.0);
        card = wp_shape(card, gear + 10.0, vec2(20.0), 5.0, WP_PANEL, WP_TEAL * 0.72, dpi);
        card = over(card, WP_TEAL, dock_icon(6u, DOCK_BUTTONS[6].xy + DOCK_BUTTONS[6].zw * 0.5 + gear * 2.0).a *
                    coverage(length(gear) - 9.0, dpi));
    }
    out = over(out, card.rgb, a);
    return over(out, select(WP_BORDER, WP_TEAL, hot || is_current), a - coverage(d + 1.35, dpi));
}

fn wp_card_glow(ui: vec4<f32>, grid: vec2<f32>, cell: vec2<f32>, slot: u32, alpha: f32) -> vec4<f32> {
    if slot >= 30u { return ui; }
    let q = grid - vec2(f32(slot % 5u), f32(slot / 5u)) * cell;
    if any(q < vec2(-20.0)) || any(q > vec2(252.0, 180.0)) { return ui; }
    let d = rounded_distance(q, vec2(232.0, 160.0), 10.0);
    return over(ui, WP_TEAL, alpha * gaussian_cdf(-d / 5.75));
}

fn wp_content(below: vec4<f32>, pt: vec2<f32>, height: f32, dpi: f32) -> vec4<f32> {
    var ui = below;
    if pt.y < 108.0 { return ui; }
    if pt.y >= height - 40.0 {
        ui = over(ui, WP_BORDER, coverage(abs(pt.y - (height - 40.0)) - 0.5, dpi) * 0.6);
        ui = wp_label(ui, pt, 20.0, height - 20.0, WP_CURRENT, WP_MUTED);
        if (p.wall_ui.z & 8u) != 0u { return wp_label(ui, pt, 105.0, height - 20.0, WP_SWITCHING, WP_TEAL); }
        ui = wp_line(ui, pt - vec2(105.0, height - 20.0), 30u, WP_TEAL);
        ui = wp_line(ui, pt - vec2(432.0, height - 20.0), 32u, WP_PURPLE);
        if (p.wall_ui.z & 32u) != 0u {
            let y = height - 35.0;
            if p.wall_catalog.y > 1u {
                ui = wp_shape(ui, pt - vec2(1016.0, y), vec2(62.0, 30.0), 7.0, WP_PANEL,
                              select(WP_BORDER, WP_TEAL, p.wall_ui.y == 17u), dpi);
                ui = over(ui, WP_TEAL, wp_glyph(60u, pt - vec2(1042.0, y + 6.0)));
            }
            if p.wall_catalog.z != 0u {
                ui = wp_shape(ui, pt - vec2(1178.0, y), vec2(62.0, 30.0), 7.0, WP_PANEL,
                              select(WP_BORDER, WP_TEAL, p.wall_ui.y == 18u), dpi);
                ui = over(ui, WP_TEAL, wp_glyph(62u, pt - vec2(1204.0, y + 6.0)));
            }
            ui = wp_caption(ui, pt - vec2(1090.0, height - 20.0), W_PAG, WP_MUTED);
            ui = wp_number(ui, pt - vec2(1157.0, height - 20.0), p.wall_catalog.y, WP_TEAL);
        }
        return ui;
    }
    let top = f32(p.wall_catalog.w);
    if pt.y < top - 10.0 { return wp_toolbar(ui, pt, dpi); }
    if pt.x >= 1234.0 { return wp_scrollbar(ui, pt, dpi); }
    if p.wall_state.z == 0.0 {
        if p.wall_ui.x == 3u {
            ui = wp_label(ui, pt, 630.0 - WP_EMPTY_LIST.rect.z * 0.5, 213.0, WP_EMPTY_LIST, WP_TEXT);
            return wp_label(ui, pt, 630.0 - WP_LIST_HINT.rect.z * 0.5, 243.0, WP_LIST_HINT, WP_MUTED);
        }
        if p.wall_rows[32].z > 0u {
            return wp_line(ui, pt - vec2(630.0 - f32(p.wall_rows[32].z >> 8u) * 0.5, 244.0), 32u, WP_TEXT);
        }
        return wp_label(ui, pt, 630.0 - WP_NO_RESULTS.rect.z * 0.5, 244.0, WP_NO_RESULTS, WP_TEXT);
    }
    let grid = pt - vec2(20.0, top - p.wall_grid.x);
    let cell = vec2(245.0, 173.0);
    ui = wp_card_glow(ui, grid, cell, p.wall_glows.x, 0.50);
    if p.wall_glows.y != p.wall_glows.x {
        ui = wp_card_glow(ui, grid, cell, p.wall_glows.y, 0.30);
    }
    if grid.y < -6.0 || grid.x < -6.0 || grid.x >= 1220.0 { return ui; }
    let col = floor((grid.x + 6.0) / cell.x);
    let row = floor((grid.y + 6.0) / cell.y);
    let slot = u32(row * 5.0 + col);
    if col >= 5.0 || slot >= u32(p.wall_grid.y) { return ui; }
    return wp_card(ui, grid - vec2(col, row) * cell, slot, dpi);
}

fn wp_settings(below: vec4<f32>, pt: vec2<f32>, dpi: f32) -> vec4<f32> {
    var ui = wp_shape(below, pt - vec2(20.0, 51.0), vec2(284.0, 94.0), 10.0, WP_PANEL, WP_BORDER, dpi);
    ui = wp_label(ui, pt, 36.0, 72.0, WP_SETTINGS, WP_TEXT);
    ui = wp_label(ui, pt, 36.0, 104.0, WP_DIM, WP_MUTED);
    ui = wp_number(ui, pt - vec2(268.0, 104.0), u32(round(p.wall_state.w * 100.0)), WP_TEAL);
    ui = over(ui, WP_TEAL, wp_glyph(37u, pt - vec2(282.0, 95.0)));
    let x = p.wall_state.w * (252.0 / 0.95);
    let line = pt - vec2(36.0, 124.0);
    ui = over(ui, WP_BORDER, coverage(rounded_distance(line, vec2(252.0, 5.0), 2.5), dpi));
    if x > 0.0 { ui = over(ui, WP_TEAL, coverage(rounded_distance(line, vec2(x, 5.0), 2.5), dpi)); }
    return over(ui, WP_TEAL, coverage(length(line - vec2(x, 2.5)) - 6.0, dpi));
}

fn wp_details(below: vec4<f32>, pt: vec2<f32>, height: f32, dpi: f32) -> vec4<f32> {
    let q = pt - vec2(897.0, 198.0);
    let size = vec2(342.0, height - 242.0);
    let d = rounded_distance(q - vec2(0.0, 6.0), size, 12.0);
    let shadow = over(below, vec3(0.0), 0.86 * gaussian_cdf(-d / 9.0));
    if any(q < vec2(-1.0)) || any(q > size + 1.0) { return shadow; }
    var ui = wp_shape(shadow, q, size, 12.0, WP_PANEL, WP_BORDER * 1.38, dpi);
    ui = wp_caption(ui, q - vec2(13.0, 23.0), W_PROPIEDADES, WP_TEAL);
    ui = wp_line(ui, q - vec2(13.0, 49.0), 42u, WP_MUTED);
    let close = q - vec2(316.0, 26.0);
    ui = over(ui, select(WP_MUTED, WP_TEXT, p.wall_ui.y == 2000001u), centered_mask(CONTROL_SPRITES[2], close));
    let i = i32(floor((pt.y - 262.0) / 39.0));
    if i >= 0 && i < i32(p.wall_details.y) {
        let line = 34u + u32(i);
        let y = 262.0 + 39.0 * f32(i);
        let id = 2000010u + p.wall_rows[line].w * 2u;
        ui = wp_shape(ui, pt - vec2(908.0, y + 2.0), vec2(319.0, 35.0), 7.0, CHROME_FILL, WP_BORDER, dpi);
        ui = wp_line(ui, pt - vec2(915.0, y + 19.0), line, WP_TEXT);
        ui = wp_pill(ui, pt, vec4(1142.0, y + 4.0, 37.0, 31.0), id, false, W_MENOS, dpi);
        ui = wp_pill(ui, pt, vec4(1187.0, y + 4.0, 37.0, 31.0), id + 1u, false, W_MAS, dpi);
    }
    let y = height - 85.0;
    if p.wall_details.z > 0u {
        ui = wp_pill(ui, pt, vec4(908.0, y, 36.0, 34.0), 2000004u, false, W_ANTERIOR, dpi);
    }
    if p.wall_details.w > 0u {
        ui = wp_pill(ui, pt, vec4(981.0, y, 36.0, 34.0), 2000005u, false, W_SIGUIENTE, dpi);
    }
    if p.wall_details.y > 0u {
        ui = wp_pill(ui, pt, vec4(1032.0, y, 94.0, 34.0), 2000002u, false, W_RESTABLECER, dpi);
        ui = wp_pill(ui, pt, vec4(1130.0, y, 97.0, 34.0), 2000003u, true, W_APLICAR, dpi);
    }
    return ui;
}

fn draw_wallpapers(pos: vec2<f32>) -> vec4<f32> {
    let dpi = p.wall_rect.z;
    let size = vec2(1260.0, p.wall_rect.w);
    let center = (pos - p.wall_rect.xy) / dpi - size * 0.5;
    var ui = vec4(0.0);
    if any(abs(center) > size * 0.52 + vec2(125.0, 150.0)) { return ui; }
    let layers = p.wall_crt;
    let screen = layers.panel;
    if screen.z > 0.0 {
        let pt = center / screen.xy + size * 0.5;
        let a = tube_edge(pt, size, 20.0, screen.xy, dpi);
        ui = over(ui, vec3(0.0), 0.95 * gaussian_cdf(-rounded_distance(pt - vec2(0.0, 25.0), size, 20.0) / 40.0));
        if a > 0.0 {
            let base = vec4(CHROME_FILL, 1.0);
            var panel = base;
            let part = select(select(p.wall_fade.w, p.wall_fade.z, pt.y < 107.0), p.wall_fade.y, pt.y < 59.0);
            if part > 0.0 {
                let travel = select(14.0, -6.0, layers.beam.w > 0.0);
                let content_pt = pt - vec2(0.0, travel * (1.0 - part));
                if pt.y < 59.0 { panel = mix(base, wp_header(base, content_pt, dpi), part); }
                else if pt.y < 107.0 { panel = mix(base, wp_tabs(base, content_pt, dpi), part); }
                else { panel = mix(base, wp_content(base, content_pt, size.y, dpi), part); }
            }
            if p.wall_details.x != 0u && pt.x > 860.0 && pt.y > 160.0 {
                panel = wp_details(panel, pt, size.y, dpi);
            }
            if (p.wall_ui.z & 2u) != 0u && all(pt < vec2(311.0, 150.0)) { panel = wp_settings(panel, pt, dpi); }
            let stroke = rounded_distance(pt - 1.0, size - 2.0, 20.0);
            if abs(stroke) < 2.5 {
                panel = over(panel, dock_ramp(fract((pt.x - p.dock_phase.x) / 2400.0), 1.0),
                    coverage(abs(stroke) - 0.9, dpi) * select(1.0, p.wall_fade.y, layers.beam.w > 0.0));
            }
            ui = over(ui, panel.rgb, a);
        }
    }
    // In WebView2 Wallpapers has no dark/blurred backdrop; the CSS fade
    // applies to the panel and shadow together, below the separate light.
    return tube_light(ui * (screen.z * p.wall_fade.x), center, size, dpi, 20.0, layers);
}
