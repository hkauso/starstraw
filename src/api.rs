//! Responds to API requests
use crate::database::Database;
use crate::model::{
    GrantSkill, GrantTitle, ProfileCreate, ProfileLogin, RevokeSkill, SkillManager, SkillName,
    StrawError,
};
use axum::http::HeaderMap;
use dorsal::DefaultReturn;

use axum::response::IntoResponse;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::cookie::CookieJar;

pub fn routes(database: Database) -> Router {
    Router::new()
        // admin
        .route("/spirit/:username/grant", post(grant_skill_request))
        .route("/spirit/:username/revoke", post(revoke_skill_request))
        .route("/spirit/:username/seed", post(grant_title_request))
        // me
        .route("/me", get(my_stats_request))
        // initial account
        .route("/start", post(create_profile_request))
        .route("/return", post(login_request))
        // ...
        .with_state(database)
}

/// [`Database::create_profile`]
pub async fn create_profile_request(
    jar: CookieJar,
    State(database): State<Database>,
    Json(props): Json<ProfileCreate>,
) -> impl IntoResponse {
    if let Some(_) = jar.get("__Secure-Token") {
        return (
            HeaderMap::new(),
            serde_json::to_string(&DefaultReturn {
                success: false,
                message: StrawError::NotAllowed.to_string(),
                payload: (),
            })
            .unwrap(),
        );
    }

    let res = match database.create_profile(props.username).await {
        Ok(r) => r,
        Err(e) => {
            return (
                HeaderMap::new(),
                serde_json::to_string(&DefaultReturn {
                    success: false,
                    message: e.to_string(),
                    payload: (),
                })
                .unwrap(),
            );
        }
    };

    // return
    let mut headers = HeaderMap::new();

    headers.insert(
        "Set-Cookie",
        format!(
            "__Secure-Token={}; SameSite=Lax; Secure; Path=/; HostOnly=true; HttpOnly=true; Max-Age={}",
            res,
            60* 60 * 24 * 365
        )
        .parse()
        .unwrap(),
    );

    (
        headers,
        serde_json::to_string(&DefaultReturn {
            success: true,
            message: res.clone(),
            payload: (),
        })
        .unwrap(),
    )
}

/// [`Database::get_profile_by_unhashed_st`]
pub async fn login_request(
    State(database): State<Database>,
    Json(props): Json<ProfileLogin>,
) -> impl IntoResponse {
    if let Err(e) = database.get_profile_by_unhashed(props.id.clone()).await {
        return (
            HeaderMap::new(),
            serde_json::to_string(&DefaultReturn {
                success: false,
                message: e.to_string(),
                payload: (),
            })
            .unwrap(),
        );
    };

    // return
    let mut headers = HeaderMap::new();

    headers.insert(
        "Set-Cookie",
        format!(
            "__Secure-Token={}; SameSite=Lax; Secure; Path=/; HostOnly=true; HttpOnly=true; Max-Age={}",
            props.id,
            60* 60 * 24 * 365
        )
        .parse()
        .unwrap(),
    );

    (
        headers,
        serde_json::to_string(&DefaultReturn {
            success: true,
            message: props.id,
            payload: (),
        })
        .unwrap(),
    )
}

/// [`SkillManager::get_stats`]
pub async fn my_stats_request(
    jar: CookieJar,
    State(database): State<Database>,
) -> impl IntoResponse {
    // get user from token
    let auth_user = match jar.get("__Secure-Token") {
        Some(c) => match database
            .get_profile_by_unhashed(c.value_trimmed().to_string())
            .await
        {
            Ok(ua) => ua,
            Err(e) => {
                return Json(DefaultReturn {
                    success: false,
                    message: e.to_string(),
                    payload: None,
                });
            }
        },
        None => {
            return Json(DefaultReturn {
                success: false,
                message: StrawError::NotAllowed.to_string(),
                payload: None,
            });
        }
    };

    // create manager
    let manager = SkillManager(auth_user.skills);

    // return
    Json(DefaultReturn {
        success: true,
        message: String::new(),
        payload: Some(manager.get_stats()),
    })
}

/// [`SkillManager::push`]
pub async fn grant_skill_request(
    jar: CookieJar,
    Path(username): Path<String>,
    State(database): State<Database>,
    Json(props): Json<GrantSkill>,
) -> impl IntoResponse {
    // get user from token
    let auth_user = match jar.get("__Secure-Token") {
        Some(c) => match database
            .get_profile_by_unhashed(c.value_trimmed().to_string())
            .await
        {
            Ok(ua) => ua,
            Err(e) => {
                return Json(DefaultReturn {
                    success: false,
                    message: e.to_string(),
                    payload: None,
                });
            }
        },
        None => {
            return Json(DefaultReturn {
                success: false,
                message: StrawError::NotAllowed.to_string(),
                payload: None,
            });
        }
    };

    // check permission
    let manager = SkillManager(auth_user.skills);
    let stats = manager.get_stats();

    if stats.title != SkillName::God {
        // we must have the "God" title to edit other users
        return Json(DefaultReturn {
            success: false,
            message: StrawError::NotAllowed.to_string(),
            payload: None,
        });
    }

    // get other user
    let other_user = match database.get_profile_by_username(username.clone()).await {
        Ok(ua) => ua,
        Err(e) => {
            return Json(DefaultReturn {
                success: false,
                message: e.to_string(),
                payload: None,
            });
        }
    };

    let mut manager = SkillManager(other_user.skills);

    // grant skill
    if let Err(e) = manager.push(props.skill) {
        return Json(DefaultReturn {
            success: false,
            message: e.to_string(),
            payload: None,
        });
    }

    // push update
    // TODO: try not to clone
    if let Err(e) = database
        .edit_profile_skills_by_name(username, manager.0.clone())
        .await
    {
        return Json(DefaultReturn {
            success: false,
            message: e.to_string(),
            payload: None,
        });
    }

    // return
    Json(DefaultReturn {
        success: true,
        message: "Acceptable".to_string(),
        payload: Some(manager.0),
    })
}

/// [`SkillManager::remove`]
pub async fn revoke_skill_request(
    jar: CookieJar,
    Path(username): Path<String>,
    State(database): State<Database>,
    Json(props): Json<RevokeSkill>,
) -> impl IntoResponse {
    // get user from token
    let auth_user = match jar.get("__Secure-Token") {
        Some(c) => match database
            .get_profile_by_unhashed(c.value_trimmed().to_string())
            .await
        {
            Ok(ua) => ua,
            Err(e) => {
                return Json(DefaultReturn {
                    success: false,
                    message: e.to_string(),
                    payload: None,
                });
            }
        },
        None => {
            return Json(DefaultReturn {
                success: false,
                message: StrawError::NotAllowed.to_string(),
                payload: None,
            });
        }
    };

    // check permission
    let manager = SkillManager(auth_user.skills);
    let stats = manager.get_stats();

    if stats.title != SkillName::God {
        // we must have the "God" title to edit other users
        return Json(DefaultReturn {
            success: false,
            message: StrawError::NotAllowed.to_string(),
            payload: None,
        });
    }

    // get other user
    let other_user = match database.get_profile_by_username(username.clone()).await {
        Ok(ua) => ua,
        Err(e) => {
            return Json(DefaultReturn {
                success: false,
                message: e.to_string(),
                payload: None,
            });
        }
    };

    let mut manager = SkillManager(other_user.skills);

    // revoke skill
    if let Err(e) = manager.remove(props.skill) {
        return Json(DefaultReturn {
            success: false,
            message: e.to_string(),
            payload: None,
        });
    }

    // push update
    // TODO: try not to clone
    if let Err(e) = database
        .edit_profile_skills_by_name(username, manager.0.clone())
        .await
    {
        return Json(DefaultReturn {
            success: false,
            message: e.to_string(),
            payload: None,
        });
    }

    // return
    Json(DefaultReturn {
        success: true,
        message: "Acceptable".to_string(),
        payload: Some(manager.0),
    })
}

/// [`SkillManager::title`]
pub async fn grant_title_request(
    jar: CookieJar,
    Path(username): Path<String>,
    State(database): State<Database>,
    Json(props): Json<GrantTitle>,
) -> impl IntoResponse {
    // get user from token
    let auth_user = match jar.get("__Secure-Token") {
        Some(c) => match database
            .get_profile_by_unhashed(c.value_trimmed().to_string())
            .await
        {
            Ok(ua) => ua,
            Err(e) => {
                return Json(DefaultReturn {
                    success: false,
                    message: e.to_string(),
                    payload: None,
                });
            }
        },
        None => {
            return Json(DefaultReturn {
                success: false,
                message: StrawError::NotAllowed.to_string(),
                payload: None,
            });
        }
    };

    // check permission
    let manager = SkillManager(auth_user.skills);
    let stats = manager.get_stats();

    if stats.title != SkillName::God {
        // we must have the "God" title to edit other users
        return Json(DefaultReturn {
            success: false,
            message: StrawError::NotAllowed.to_string(),
            payload: None,
        });
    }

    // get other user
    let other_user = match database.get_profile_by_username(username.clone()).await {
        Ok(ua) => ua,
        Err(e) => {
            return Json(DefaultReturn {
                success: false,
                message: e.to_string(),
                payload: None,
            });
        }
    };

    let mut manager = SkillManager(other_user.skills);

    // set title
    if let Err(e) = manager.title(props.title.into()) {
        return Json(DefaultReturn {
            success: false,
            message: e.to_string(),
            payload: None,
        });
    }

    // push update
    // TODO: try not to clone
    if let Err(e) = database
        .edit_profile_skills_by_name(username, manager.0.clone())
        .await
    {
        return Json(DefaultReturn {
            success: false,
            message: e.to_string(),
            payload: None,
        });
    }

    // return
    Json(DefaultReturn {
        success: true,
        message: "Acceptable".to_string(),
        payload: Some(manager.0),
    })
}

// general
pub async fn not_found() -> impl IntoResponse {
    Json(DefaultReturn::<u16> {
        success: false,
        message: String::from("Path does not exist"),
        payload: 404,
    })
}

// auth
#[derive(serde::Deserialize)]
pub struct CallbackQueryProps {
    pub uid: String, // this uid will need to be sent to the client as a token
}

pub async fn callback_request(Query(params): Query<CallbackQueryProps>) -> impl IntoResponse {
    // return
    (
        [
            ("Content-Type".to_string(), "text/html".to_string()),
            (
                "Set-Cookie".to_string(),
                format!(
                    "__Secure-Token={}; SameSite=Lax; Secure; Path=/; HostOnly=true; HttpOnly=true; Max-Age={}",
                    params.uid,
                    60 * 60 * 24 * 365
                ),
            ),
        ],
        "<head>
            <meta http-equiv=\"Refresh\" content=\"0; URL=/\" />
        </head>"
    )
}

pub async fn logout_request(jar: CookieJar) -> impl IntoResponse {
    // check for cookie
    if let Some(_) = jar.get("__Secure-Token") {
        return (
            [
                ("Content-Type".to_string(), "text/plain".to_string()),
                (
                    "Set-Cookie".to_string(),
                   "__Secure-Token=refresh; SameSite=Strict; Secure; Path=/; HostOnly=true; HttpOnly=true; Max-Age=0".to_string(),
                ),
            ],
            "You have been signed out. You can now close this tab.",
        );
    }

    // return
    (
        [
            ("Content-Type".to_string(), "text/plain".to_string()),
            ("Set-Cookit".to_string(), String::new()),
        ],
        "Failed to sign out of account.",
    )
}
