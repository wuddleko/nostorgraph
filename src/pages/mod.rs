mod assets;
mod fetch_slot;
mod home;
mod layout;
#[cfg(test)]
mod nojs_test;
mod relays;
mod seal_row;
mod sealed;
mod static_pages;
mod title_link;

pub use assets::{
    cache_control_for_request, editor_mode_js, entity_tag_for_path, home_css, home_js,
    home_noscript_css, if_none_match_matches, logo_png, nostr_js, notice_css, parser_js,
    parser_wasm, pdf_js, pdf_license, pdf_wasm, post_css, post_js, post_noscript_css, relays_js,
    robots_txt, secp256k1_js, theme_js, warm, writemark_css, writemark_js, writemark_live_js,
};
pub use home::{
    assign_tor_circuits, index, nojs_create_post, nojs_index, nostr_publish, remember_short_link,
    remember_title_link, tor_circuits,
};
pub(crate) use layout::not_found_html;
pub use relays::{nojs_relays_page, nojs_relays_update, relays_page};
pub use sealed::{
    nojs_sealed_locator_view, nojs_sealed_view, root_link_view, root_sealed_view,
    sealed_locator_view, sealed_view, title_view,
};
pub use static_pages::{about_page, api_page, legal_page, markup_page, nojs_static_page};

#[cfg(test)]
pub use assets::{
    cache_control_for_path, EDITOR_MODE_JS_PATH, HOME_CSS_PATH, HOME_JS_PATH,
    HOME_NOSCRIPT_CSS_PATH, LOGO_PNG_PATH, NOSTR_JS_PATH, NOTICE_CSS_PATH, PAGE_JS_PATH,
    PAGE_WASM_PATH, PDF_JS_PATH, PDF_LICENSE_PATH, PDF_WASM_PATH, POST_CSS_PATH, POST_JS_PATH,
    POST_NOSCRIPT_CSS_PATH, RELAYS_JS_PATH, SECP256K1_JS_PATH, THEME_JS_PATH, WRITEMARK_CSS_PATH,
    WRITEMARK_JS_PATH, WRITEMARK_LIVE_JS_PATH,
};
#[cfg(test)]
pub(crate) use assets::{post_scripts, PAGE_JS, PDF_JS};
#[cfg(test)]
pub(crate) use fetch_slot::{
    release_public_fetch, try_acquire_public_fetch, PublicFetchSlot, PublicPublishSlot,
    MAX_PUBLIC_FETCHES, MAX_PUBLIC_PUBLISHES,
};
#[cfg(test)]
pub use home::NostrPublishBody;
#[cfg(test)]
pub(crate) use home::{
    create_location, create_location_with_cookie, note_and_relays_for_publish,
    relays_for_circuit_pass, tab_relay_timeout_ms, NewPost, TorCircuitsBody,
};
#[cfg(test)]
pub(crate) use layout::{fill_layout, mode_href, mode_label, post_description};
#[cfg(test)]
pub(crate) use sealed::{fetch_sealed_by_id, sealed_shell};
#[cfg(test)]
pub(crate) use static_pages::{build_static_page, BuiltPage};

#[cfg(test)]
use rocket::http::{ContentType, Status};

#[cfg(test)]
use home::{
    home_context, home_error_message, long_seal_href, nojs_error_code, nojs_publish_failure_page,
    nojs_publish_relays, nojs_publish_slot, nojs_sealed_location, publish_nojs_note_with_cookie,
    publish_result, publish_sealed_note, short_seal_href,
};
#[cfg(test)]
use nojs_test::{
    nojs_fetch_hints_seen, nojs_publish_relays_seen, note_nojs_publish_relays,
    set_nojs_test_accepted, set_nojs_test_fetch, set_shared_nojs_accepted, NojsTestFetch,
    NOJS_FETCH_HINTS, NOJS_RECORD_HINTS,
};

pub(crate) use crate::relay_set::public_relays;
#[cfg(test)]
pub(crate) use client::relays_to_assign;

#[cfg(test)]
#[path = "../../test/pages.rs"]
mod tests;
