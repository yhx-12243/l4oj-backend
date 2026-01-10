use core::{future::ready, mem};

use axum::{
    Router,
    extract::Query,
    routing::{get, get_service},
};
use compact_str::CompactString;
use futures_util::TryStreamExt;
use http::{StatusCode, response::Parts};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use tokio_postgres::Client;

use crate::{
    libs::{
        db::{DBResult, get_connection},
        preference::server::Pagination,
        request::{RawPayload, Repult},
        response::JkmxJsonResponse,
        serde::SliceMap,
    },
    models::{discussion::Discussion, problem::Problem, user::User},
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
                ("WC 2026", 1_770_336_000_000),
                ("IMO 2026", 1_784_075_400_000),
                ("IOI 2026", 1_786_215_600_000),
            ].as_slice()),
        }
    }
}

#[repr(transparent)]
#[derive(Serialize)]
struct FriendLinks<'a> {
    links: &'a SliceMap<&'static str, &'static str>,
}

mod links;

#[derive(Deserialize)]
struct HomepageRequest {
    locale: Option<CompactString>,
}

const ANNOUNCEMENT_IDS: [u32; 4] = [1, 2, 3, 4];

#[derive(Serialize)]
struct Inner {
    meta: Problem,
    title: CompactString,
}

const N_PROBLEMS: usize = Pagination::default().homepage_problem_list as usize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HomepageResponse<'a> {
    announcements: Vec<Discussion>,
    hitokoto: HitokotoConfig,
    countdown: Countdowns,
    friend_links: FriendLinks<'a>,
    top_users: Vec<User>,
    latest_updated_problems: SmallVec<[Inner; N_PROBLEMS]>,
}

async fn get_latest_updated_problems(locale: Option<&str>, db: &mut Client) -> DBResult<SmallVec<[Inner; N_PROBLEMS]>> {
    const SQL: &str = "select pid, is_public, public_at, owner, pcontent, sub, ac, submittable, jb from lean4oj.problems where is_public = true order by public_at desc limit $1";

    let stmt = db.prepare_static(SQL.into()).await?;
    #[allow(clippy::cast_possible_wrap)]
    let stream = db.query_raw(&stmt, [N_PROBLEMS as i64]).await?;
    stream.and_then(|row| ready(try {
        let mut problem = Problem::try_from(row)?;
        let content = mem::take(&mut problem.content);
        Inner {
            meta: problem,
            title: content.apply_owned(locale).map_or_default(|x| x.title),
        }
    })).try_collect().await
}

async fn get_homepage(req: Repult<Query<HomepageRequest>>) -> JkmxJsonResponse {
    let Query(HomepageRequest { locale }) = req?;

    let mut conn = get_connection().await?;
    let top_users = User::list(
        0,
        const { Pagination::default().homepage_user_list.into() },
        &mut conn,
    ).await?;

    let mut announcements = Discussion::by_ids(ANNOUNCEMENT_IDS.into_iter(), &mut conn).await?;
    for d in &mut announcements { d.backdoor(locale.as_deref()); }
    let links = links::friend_links(locale.as_deref());
    let res = HomepageResponse {
        announcements,
        hitokoto: const { HitokotoConfig::default() },
        countdown: const { Countdowns::default() },
        friend_links: FriendLinks { links: SliceMap::from_slice(&links) },
        top_users,
        latest_updated_problems: get_latest_updated_problems(locale.as_deref(), &mut conn).await?,
    };

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

const fn get_homepage_settings(header: &'static Parts) -> RawPayload {
    RawPayload { header, body: br#"{"announcementDiscussions":[],"settings":{"notice":{"contents":{}},"announcements":{"items":{}}}}"# }
}

pub fn router(header: &'static Parts) -> Router {
    Router::new()
        .route("/getHomepage", get(get_homepage))
        .route("/getHomepageSettings", get_service(get_homepage_settings(header)))
}
