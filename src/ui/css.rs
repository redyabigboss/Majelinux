const TOKENS: [(&str, &str); 8] = [
    ("$R_SM", "12px"),
    ("$R_ITEM", "7px"),
    ("$R_MD", "16px"),
    ("$R_LG", "24px"),
    ("$R_XL", "28px"),
    ("$R_HERO", "48px"),
    ("$R_PILL", "999px"),
    ("$RING", "inset 0 0 0 2px"),
];

pub fn build() -> String {
    let mut css = TEMPLATE.to_string();
    for (token, value) in TOKENS {
        css = css.replace(token, value);
    }
    css
}

const TEMPLATE: &str = r##"
@define-color md_error #ffb4ab;
@define-color md_surface #1a1112;
@define-color md_surface_container_low #221719;
@define-color md_surface_container #261b1d;
@define-color md_surface_container_high #2f2224;
@define-color md_surface_container_highest #382a2d;
@define-color md_on_surface #f0dedf;
@define-color md_on_surface_variant #d7c1c3;
@define-color md_primary #ffb2bb;
@define-color md_on_primary #561d28;
@define-color md_primary_container #5b2733;
@define-color md_on_primary_container #ffd9dd;
@define-color md_secondary_container #4d3439;
@define-color md_on_secondary_container #ffd9dd;
@define-color md_outline_variant #524344;
@define-color md_hover #3a2c2f;
@define-color md_pressed #463538;
@define-color md_primary_hover #ffc4cb;
@define-color md_primary_pressed #e99aa4;
@define-color md_selected_hover #6d3542;
@define-color md_on_disabled #7d6a6c;
@define-color md_warning #ffd0a8;
@define-color md_ok #a8d5a2;
@define-color theme_bg_color @md_surface;
@define-color theme_fg_color @md_on_surface;
@define-color theme_base_color @md_surface_container;
@define-color theme_text_color @md_on_surface;
@define-color theme_selected_bg_color @md_primary_container;
@define-color theme_selected_fg_color @md_on_primary_container;
@define-color insensitive_fg_color @md_on_disabled;

* { font-family: "Google Sans", Sans; font-size: 14px; }
window, dialog, messagedialog { background: @md_surface; color: @md_on_surface; }
window.popup, window.popup decoration {
    background: none; background-color: transparent; border: none; border-radius: 0;
    box-shadow: none; margin: 0; padding: 0;
}
dialog .dialog-action-area { padding: 10px; }
separator { background-color: @md_outline_variant; min-height: 1px; }

.page { padding: 22px 26px 16px 18px; }
.navigation-rail { background: #211518; padding: 18px 8px 10px 8px; }
.brand { color: @md_on_surface; font-size: 15px; font-weight: 800; }
.caption { color: @md_on_surface_variant; font-size: 12px; }
.body { color: @md_on_surface; font-size: 14px; }
.eyebrow { color: @md_primary; font-size: 12px; font-weight: 700; letter-spacing: 1px; }
.page-title { color: @md_on_surface; font-size: 30px; font-weight: 500; }
.hero-title { color: @md_on_surface; font-size: 34px; font-weight: 700; }
.section-title { color: @md_on_surface; font-size: 18px; font-weight: 600; }
.warning { color: @md_warning; font-size: 13px; }
.group-title { color: @md_primary; font-size: 13px; font-weight: 600; padding: 2px 12px; }
.settings-group { margin-bottom: 10px; }
.preference-title { color: @md_on_surface; font-size: 14px; font-weight: 600; }
.proton-name { color: @md_on_surface; font-size: 14px; font-weight: 600; }
.proton-path { color: @md_on_surface_variant; font-size: 12px; font-weight: 400; }

.hero {
    background-image: linear-gradient(135deg, #35212a 0%, #241619 52%, #2b1a20 100%);
    border-radius: $R_XL $R_XL $R_HERO $R_XL;
    padding: 22px 26px 24px 26px;
}
.about-hero {
    background-image: linear-gradient(135deg, #3a232c 0%, #241619 55%, #2d1b22 100%);
    border-radius: $R_LG;
    padding: 20px 22px;
}
.version-chip {
    background-color: @md_primary_container; color: @md_on_primary_container;
    border-radius: $R_PILL; padding: 4px 12px; font-size: 12px; font-weight: 700;
}
.repo-url { color: @md_on_surface_variant; font-family: monospace; font-size: 12px; }
.section-card { background-color: @md_surface_container_low; border-radius: $R_LG; padding: 16px 18px; }
.settings-surface { background-color: @md_surface_container_low; border-radius: $R_LG; padding: 8px; }
.preference-row { background-color: @md_surface_container; border-radius: $R_MD; padding: 12px 16px; }
.status-chip {
    background-color: @md_surface_container_highest; color: @md_primary;
    border-radius: $R_PILL; padding: 3px 11px; font-size: 12px; font-weight: 600;
}
.status-chip.chip-ready { color: @md_ok; }
.status-chip.chip-bad { color: @md_error; }

button {
    background-image: none; background-color: @md_surface_container_highest; color: @md_on_surface;
    border: none; border-image: none; border-radius: $R_PILL;
    min-height: 20px; padding: 9px 16px;
    box-shadow: $RING transparent; text-shadow: none; -gtk-icon-shadow: none;
    font-weight: 600; outline: none;
    transition: background-color 150ms ease-out;
}
button:hover { background-color: @md_hover; }
button:active { background-color: @md_pressed; }
button:focus { box-shadow: $RING alpha(@md_primary, 0.6); }
button.primary { background-color: @md_primary; color: @md_on_primary; font-weight: 700; }
button.primary:hover { background-color: @md_primary_hover; }
button.primary:active { background-color: @md_primary_pressed; }
button.primary:focus { box-shadow: $RING alpha(@md_on_primary, 0.55); }
button.play-button { font-size: 21px; font-weight: 700; min-height: 34px; padding: 13px 30px; }
button.play-button label { font-weight: 700; }
button.destructive { background-color: #7d3a44; color: #ffe0e4; }
button.destructive:hover { background-color: #8f4550; }
button.destructive:active { background-color: #a05360; }
button.text-button { background-color: transparent; color: @md_primary; padding: 7px 12px; }
button.text-button:hover { background-color: alpha(@md_primary, 0.10); }
button.text-button:active { background-color: alpha(@md_primary, 0.18); }
button.icon-button { padding: 7px 11px; }
button.segment { color: @md_on_surface_variant; padding: 7px 18px; min-height: 18px; }
button.segment.seg-active { background-color: @md_primary; color: @md_on_primary; font-weight: 700; }
button.segment.seg-active:focus { box-shadow: $RING alpha(@md_on_primary, 0.55); }
button.navigation-item {
    background-color: transparent; color: @md_on_surface_variant; border-radius: $R_LG;
    padding: 8px 2px; min-width: 74px; min-height: 58px; font-size: 12px;
}
button.navigation-item:hover { background-color: @md_surface_container_high; }
button.navigation-item:active { background-color: @md_pressed; }
button.navigation-item.nav-selected { background-color: @md_secondary_container; color: @md_on_secondary_container; font-weight: 700; }
button.navigation-item.nav-selected:hover { background-color: @md_selected_hover; }
button:disabled { background-color: alpha(@md_on_surface, 0.08); color: alpha(@md_on_surface, 0.38); box-shadow: $RING transparent; }
button.text-button:disabled { background-color: transparent; }

.proton-option { background-color: @md_surface_container; border-radius: $R_MD; padding: 12px 16px; font-weight: 400; box-shadow: $RING transparent; }
.proton-option.proton-selected { background-color: @md_secondary_container; box-shadow: $RING alpha(@md_primary, 0.45); }
button.proton-option:hover { background-color: @md_surface_container_high; }
button.proton-option:active { background-color: @md_pressed; }
button.proton-option:focus { box-shadow: $RING alpha(@md_primary, 0.6); }
button.proton-option.proton-selected:hover { background-color: @md_selected_hover; }
button.proton-option:disabled { background-color: @md_surface_container; box-shadow: $RING transparent; }
button.proton-option.proton-selected:disabled { background-color: @md_secondary_container; box-shadow: $RING alpha(@md_primary, 0.45); }

entry {
    background-color: @md_surface_container_high; color: @md_on_surface;
    border: 2px solid transparent; border-radius: $R_SM;
    min-height: 20px; padding: 8px 11px; box-shadow: none; caret-color: @md_primary;
}
entry:focus { border-color: @md_primary; }
entry:disabled { color: alpha(@md_on_surface, 0.40); }
entry.mono { font-family: monospace; }
entry selection, textview text selection { background-color: alpha(@md_primary, 0.35); color: @md_on_surface; }

scrolledwindow { border: none; }
viewport, scrolledwindow > viewport { background-color: transparent; border: none; }
scrolledwindow.log-surface { background-color: @md_surface_container_low; border-radius: $R_MD; }
scrolledwindow.log-surface scrollbar.vertical { margin: 10px 0; }
textview, textview text { background-color: transparent; color: @md_on_surface_variant; font-family: monospace; font-size: 12px; }
scrollbar { background: transparent; border: none; min-width: 8px; min-height: 8px; }
scrollbar trough { background: transparent; border: none; }
scrollbar button { background: transparent; border: none; min-width: 0; min-height: 0; padding: 0; }
scrollbar slider { background-color: alpha(@md_primary, 0.34); border-radius: $R_PILL; min-width: 4px; min-height: 24px; margin: 2px; }
scrollbar slider:hover { background-color: alpha(@md_primary, 0.55); }
scrollbar slider:active { background-color: alpha(@md_primary, 0.80); }

menu, .menu, .context-menu {
    background-color: @md_surface_container_high; color: @md_on_surface;
    border: 1px solid @md_outline_variant; border-radius: $R_SM; padding: 4px; margin: 0; box-shadow: none;
}
popover, popover.background {
    background-color: @md_surface_container_high; color: @md_on_surface;
    border: 1px solid @md_outline_variant; border-radius: $R_SM; padding: 4px; box-shadow: none;
}
menuitem { background-color: transparent; color: @md_on_surface; padding: 8px 12px; min-height: 20px; border-radius: $R_ITEM; }
menuitem:hover, menuitem:selected { background-color: @md_primary_container; color: @md_on_primary_container; }
menuitem:disabled { color: alpha(@md_on_surface, 0.38); }
menu separator, .menu separator { background-color: @md_outline_variant; min-height: 1px; margin: 4px 8px; }
"##;
