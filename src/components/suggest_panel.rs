use dioxus::prelude::*;
use std::collections::BTreeSet;
use surfacewatch_core::{
    discover,
    model::{Engagement, Target},
    rea, setup,
};

/// Pages found in the target's code, to add to its crawl.
///
/// The scan starts when the panel opens: a throwaway browser loads the target
/// and REA reads the routes its scripts declare.
#[component]
pub fn SuggestPanel(
    engagement: Engagement,
    target: Target,
    on_close: EventHandler<()>,
    /// Called with the number of pages added.
    on_added: EventHandler<usize>,
) -> Element {
    let mut picked = use_signal(BTreeSet::<String>::new);
    let mut error = use_signal(|| Option::<String>::None);

    let scan = use_resource({
        let (e, t) = (engagement.clone(), target.clone());
        move || {
            let (e, t) = (e.clone(), t.clone());
            async move {
                let res = tokio::task::spawn_blocking(move || discover::scan(&e, &t))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r);
                if let Ok(s) = &res {
                    // New, visitable pages start ticked.
                    picked.set(
                        s.suggestions
                            .iter()
                            .filter(|x| !x.already_listed && !x.needs_value)
                            .map(|x| x.page.clone())
                            .collect(),
                    );
                }
                res
            }
        }
    });

    let add = {
        let (eid, tid) = (engagement.id.clone(), target.id.clone());
        move |_| {
            let pages: Vec<String> = picked.read().iter().cloned().collect();
            match setup::add_pages(&eid, &tid, &pages) {
                Ok(n) => on_added.call(n),
                Err(e) => error.set(Some(e)),
            }
        }
    };

    rsx! {
        section { class: "suggest",
            div { class: "hd",
                div { class: "titles",
                    h3 { "Pages found in the code" }
                    span { class: "muted small",
                        "Routes the scripts of {target.origin} declare, including pages no link points to."
                    }
                }
                button { class: "btn ghost", onclick: move |_| on_close.call(()), "Close" }
            }
            match &*scan.read() {
                None => rsx! {
                    div { class: "notice",
                        "Opening the target in a background browser and reading its code. This takes 10 to 20 seconds."
                    }
                },
                Some(Err(e)) => rsx! {
                    div { class: "notice bad-notice", "The scan did not work: {e}" }
                },
                Some(Ok(s)) => {
                    let new_count = s
                        .suggestions
                        .iter()
                        .filter(|x| !x.already_listed && !x.needs_value)
                        .count();
                    let n = picked.read().len();
                    rsx! {
                        p { class: "small",
                            "{s.routes_found} routes in the code · {new_count} new pages · "
                            span { class: "muted mono", "{s.evidence_id.clone().unwrap_or_default()}" }
                        }
                        if s.routes_found <= 2 {
                            div { class: "notice warn-notice small",
                                {
                                    format!(
                                        "Few routes found. REA {} misses routes written as template literals, which many builds use; the fix (morluto/rea#2805) ships in the next REA release. To try it now, set SURFACE_WATCH_REA to a newer REA build.",
                                        rea::TESTED_REA_VERSION,
                                    )
                                }
                            }
                        }
                        if !s.suggestions.is_empty() {
                            p { class: "muted small",
                                "Routes nested inside another route are listed without their parent (for example a settings sub-page); check those before adding."
                            }
                        }
                        div { class: "suggest-list",
                            for x in s.suggestions.iter() {
                                {
                                    let page = x.page.clone();
                                    let on = picked.read().contains(&page);
                                    let id = format!("sg-{}", page.replace(['/', '#', ':'], "_"));
                                    rsx! {
                                        label {
                                            class: if x.already_listed { "sg-row listed" } else { "sg-row" },
                                            r#for: "{id}",
                                            key: "{x.page}",
                                            input {
                                                r#type: "checkbox",
                                                id: "{id}",
                                                checked: on || x.already_listed,
                                                disabled: x.already_listed,
                                                onchange: move |e| {
                                                    let mut p = picked.write();
                                                    if e.checked() {
                                                        p.insert(page.clone());
                                                    } else {
                                                        p.remove(&page);
                                                    }
                                                },
                                            }
                                            span { class: "mono", "{x.page}" }
                                            if x.already_listed {
                                                span { class: "pill info", "in the crawl" }
                                            } else if x.needs_value {
                                                span {
                                                    class: "pill warn",
                                                    title: "Replace the :parameter with a real value before the crawl can visit it",
                                                    "needs a value"
                                                }
                                            } else {
                                                span { class: "pill ok", "new" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(e) = error() {
                            div { class: "notice bad-notice", "{e}" }
                        }
                        div { class: "row form-actions",
                            button { class: "btn", onclick: move |_| on_close.call(()), "Cancel" }
                            button { class: "btn pri", disabled: n == 0, onclick: add,
                                {format!("Add {n} page{} to the crawl", if n == 1 { "" } else { "s" })}
                            }
                        }
                    }
                }
            }
        }
    }
}
