use axum::{Router, routing::get};
use http::StatusCode;
use serde::Serialize;

use crate::{
    libs::{
        db::get_connection, preference::server::Pagination, response::JkmxJsonResponse,
        serde::SliceMap,
    },
    models::user::User,
};

#[repr(transparent)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HitokotoConfig {
    api_url: &'static str,
}

impl const Default for HitokotoConfig {
    fn default() -> Self {
        Self {
            api_url: "https://43.138.56.99/turnabout-hitokoto/api",
        }
    }
}

#[repr(transparent)]
#[derive(Serialize)]
struct Countdowns {
    items: &'static SliceMap<&'static str, u64>,
}

impl const Default for Countdowns {
    fn default() -> Self {
        Self {
            items: SliceMap::from_slice([
                ("WC 2026", 1770336000000),
                ("IMO 2026", 1784075400000),
                ("IOI 2026", 1786215600000),
            ].as_slice()),
        }
    }
}

#[repr(transparent)]
#[derive(Serialize)]
struct FriendLinks {
    links: &'static SliceMap<&'static str, &'static str,>,
}

impl const Default for FriendLinks {
    fn default() -> Self {
        Self {
            links: SliceMap::from_slice([
                ("OI Wiki", "https://oi.wiki"),
                ("Universal Online Judge", "https://uoj.ac"),
                ("LibreOJ", "https://loj.ac"),
                ("Luogu", "https://www.luogu.com.cn"),
                ("QOJ", "https://qoj.ac"),
                ("PJudge", "https://pjudge.ac"),
                ("HydroOJ", "https://hydro.ac"),
                ("Vijos", "https://vijos.org"),
                ("OIerDb", "https://oier.baoshuo.dev"),
                ("Lean Language Reference", "https://lean-lang.org/doc/reference/latest/"),
                ("Mathlib4 Documentation", "https://leanprover-community.github.io/mathlib4_docs/"),
                ("FPiL4", "https://lean-lang.org/functional_programming_in_lean/"),
                ("TPiL4", "https://lean-lang.org/theorem_proving_in_lean4/"),
            ].as_slice()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HomepageResponse {
    announcements: [!; 0],
    hitokoto: HitokotoConfig,
    countdown: Countdowns,
    friend_links: FriendLinks,
    top_users: Vec<User>,
    latest_updated_problems: [!; 0],
}

pub async fn get_homepage() -> JkmxJsonResponse {
    let mut conn = get_connection().await?;
    let top_users = User::list(
        0,
        const { Pagination::default().homepage_user_list as i64 },
        &mut conn,
    )
    .await?;

    let res = HomepageResponse {
        announcements: [],
        hitokoto: const { HitokotoConfig::default() },
        countdown: const { Countdowns::default() },
        friend_links: const { FriendLinks::default() },
        top_users,
        latest_updated_problems: [],
    };
    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

pub fn router() -> Router {
    Router::new().route("/getHomepage", get(get_homepage))
}
