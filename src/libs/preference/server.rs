use serde::Serialize;

use crate::libs::serde::UnitMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    recaptcha_enabled: bool,
    allow_user_change_username: bool,
    allow_non_privileged_user_edit_public_problem: bool,
    allow_owner_manage_problem_permission: bool,
    allow_owner_delete_problem: bool,
    discussion_default_public: bool,
    discussion_reply_default_public: bool,
    allow_everyone_create_discussion: bool,
}

impl const Default for Security {
    fn default() -> Self {
        Self {
            recaptcha_enabled: false,
            allow_user_change_username: true,
            allow_non_privileged_user_edit_public_problem: true,
            allow_owner_manage_problem_permission: true,
            allow_owner_delete_problem: true,
            discussion_default_public: true,
            discussion_reply_default_public: true,
            allow_everyone_create_discussion: true,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub homepage_user_list: u32,
    homepage_problem_list: u32,
    problem_set: u32,
    search_problems_preview: u32,
    submissions: u32,
    submission_statistics: u32,
    user_list: u32,
    user_audit_logs: u32,
    discussions: u32,
    search_discussions_preview: u32,
    discussion_replies: u32,
    discussion_replies_head: u32,
    discussion_replies_more: u32,
}

impl const Default for Pagination {
    fn default() -> Self {
        Self {
            homepage_user_list: 10,
            homepage_problem_list: 10,
            problem_set: 50,
            search_problems_preview: 7,
            submissions: 10,
            submission_statistics: 10,
            user_list: 30,
            user_audit_logs: 10,
            discussions: 10,
            search_discussions_preview: 7,
            discussion_replies: 40,
            discussion_replies_head: 20,
            discussion_replies_more: 20,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Misc {
    app_logo_for_theme: UnitMap,
    redirect_legacy_urls: bool,
    google_analytics_id: (),
    plausible_api_endpoint: (),
    render_markdown_in_user_bio: bool,
    discussion_reaction_emojis: &'static [&'static str],
    discussion_reaction_allow_custom_emojis: bool,
}

impl const Default for Misc {
    fn default() -> Self {
        Self {
            app_logo_for_theme: UnitMap {},
            redirect_legacy_urls: true,
            google_analytics_id: (),
            plausible_api_endpoint: (),
            render_markdown_in_user_bio: true,
            discussion_reaction_emojis: &["👍", "👎", "😄", "😕", "❤️", "🤔", "🤣", "🌿", "🍋", "🕊️"],
            discussion_reaction_allow_custom_emojis: true,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceConfig {
    site_name: &'static str,
    security: Security,
    pagination: Pagination,
    misc: Misc,
}

impl const Default for PreferenceConfig {
    fn default() -> Self {
        Self {
            site_name: "Lean4OJ",
            security: Security::default(),
            pagination: Pagination::default(),
            misc: Misc::default(),
        }
    }
}
