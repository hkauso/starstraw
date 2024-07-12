use crate::model::SkillSet;
use crate::model::{Profile, ProfileMetadata, Skill, SkillName, StrawError};

use dorsal::query as sqlquery;
use dorsal::utility;

pub type Result<T> = std::result::Result<T, StrawError>;

#[derive(Clone, Debug)]
pub struct ServerOptions {}

impl ServerOptions {
    /// Enable all options
    pub fn truthy() -> Self {
        Self {}
    }
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {}
    }
}

/// Database connector
#[derive(Clone)]
pub struct Database {
    pub base: dorsal::StarterDatabase,
    pub config: ServerOptions,
}

impl Database {
    /// Create a new [`Database`]
    pub async fn new(
        database_options: dorsal::DatabaseOpts,
        server_options: ServerOptions,
    ) -> Self {
        let base = dorsal::StarterDatabase::new(database_options).await;

        Self {
            base: base.clone(),
            config: server_options,
        }
    }

    /// Pull [`dorsal::DatabaseOpts`] from env
    pub fn env_options() -> dorsal::DatabaseOpts {
        use std::env::var;
        dorsal::DatabaseOpts {
            _type: match var("DB_TYPE") {
                Ok(v) => Option::Some(v),
                Err(_) => Option::None,
            },
            host: match var("DB_HOST") {
                Ok(v) => Option::Some(v),
                Err(_) => Option::None,
            },
            user: var("DB_USER").unwrap_or(String::new()),
            pass: var("DB_PASS").unwrap_or(String::new()),
            name: var("DB_NAME").unwrap_or(String::new()),
        }
    }

    /// Init database
    pub async fn init(&self) {
        // create tables
        let c = &self.base.db.client;

        let _ = sqlquery(
            "CREATE TABLE IF NOT EXISTS \"sr_profiles\" (
                id       TEXT,
                username TEXT,
                metadata TEXT,
                joined   TEXT,
                skills   TEXT
            )",
        )
        .execute(c)
        .await;
    }

    // profiles

    // GET
    /// Get a [`Profile`] by their hashed ID
    ///
    /// # Arguments:
    /// * `hashed` - `String` of the profile's hashed ID
    pub async fn get_profile_by_hashed(&self, hashed: String) -> Result<Profile> {
        // fetch from database
        let query: &str = if (self.base.db._type == "sqlite") | (self.base.db._type == "mysql") {
            "SELECT * FROM \"sr_profiles\" WHERE \"id\" = ?"
        } else {
            "SELECT * FROM \"sr_profiles\" WHERE \"id\" = $1"
        };

        let c = &self.base.db.client;
        let row = match sqlquery(query).bind::<&String>(&hashed).fetch_one(c).await {
            Ok(u) => self.base.textify_row(u).data,
            Err(_) => return Err(StrawError::Other),
        };

        // return
        Ok(Profile {
            id: row.get("id").unwrap().to_string(),
            username: row.get("username").unwrap().to_string(),
            metadata: match serde_json::from_str(row.get("metadata").unwrap()) {
                Ok(m) => m,
                Err(_) => return Err(StrawError::ValueError),
            },
            skills: match serde_json::from_str(row.get("skills").unwrap()) {
                Ok(m) => m,
                Err(_) => return Err(StrawError::ValueError),
            },
            joined: row.get("joined").unwrap().parse::<u128>().unwrap(),
        })
    }

    /// Get a user by their unhashed ID (hashes ID and then calls [`Database::get_profile_by_hashed()`])
    ///
    /// # Arguments:
    /// * `unhashed` - `String` of the user's unhashed ID
    pub async fn get_profile_by_unhashed(&self, unhashed: String) -> Result<Profile> {
        match self
            .get_profile_by_hashed(utility::hash(unhashed.clone()))
            .await
        {
            Ok(r) => Ok(r),
            Err(_) => self.get_profile_by_unhashed_st(unhashed).await,
        }
    }

    /// Get a user by their unhashed secondary token
    ///
    /// # Arguments:
    /// * `unhashed` - `String` of the user's unhashed secondary token
    pub async fn get_profile_by_unhashed_st(&self, unhashed: String) -> Result<Profile> {
        // fetch from database
        let query: &str = if (self.base.db._type == "sqlite") | (self.base.db._type == "mysql") {
            "SELECT * FROM \"sr_profiles\" WHERE \"metadata\" LIKE ?"
        } else {
            "SELECT * FROM \"sr_profiles\" WHERE \"metadata\" LIKE $1"
        };

        let c = &self.base.db.client;
        let row = match sqlquery(query)
            .bind::<&String>(&format!(
                "%\"secondary_token\":\"{}\"%",
                utility::hash(unhashed)
            ))
            .fetch_one(c)
            .await
        {
            Ok(r) => self.base.textify_row(r).data,
            Err(_) => return Err(StrawError::Other),
        };

        // return
        Ok(Profile {
            id: row.get("id").unwrap().to_string(),
            username: row.get("username").unwrap().to_string(),
            metadata: match serde_json::from_str(row.get("metadata").unwrap()) {
                Ok(m) => m,
                Err(_) => return Err(StrawError::ValueError),
            },
            skills: match serde_json::from_str(row.get("metadata").unwrap()) {
                Ok(m) => m,
                Err(_) => return Err(StrawError::ValueError),
            },
            joined: row.get("joined").unwrap().parse::<u128>().unwrap(),
        })
    }

    /// Get a user by their username
    ///
    /// # Arguments:
    /// * `username` - `String` of the user's username
    pub async fn get_profile_by_username(&self, mut username: String) -> Result<Profile> {
        username = username.to_lowercase();

        // check in cache
        let cached = self
            .base
            .cachedb
            .get(format!("sr_profile:{}", username))
            .await;

        if cached.is_some() {
            return Ok(serde_json::from_str::<Profile>(cached.unwrap().as_str()).unwrap());
        }

        // ...
        let query: &str = if (self.base.db._type == "sqlite") | (self.base.db._type == "mysql") {
            "SELECT * FROM \"sr_profiles\" WHERE \"username\" = ?"
        } else {
            "SELECT * FROM \"sr_profiles\" WHERE \"username\" = $1"
        };

        let c = &self.base.db.client;
        let row = match sqlquery(query)
            .bind::<&String>(&username)
            .fetch_one(c)
            .await
        {
            Ok(r) => self.base.textify_row(r).data,
            Err(_) => return Err(StrawError::NotFound),
        };

        // store in cache
        let user = Profile {
            id: row.get("id").unwrap().to_string(),
            username: row.get("username").unwrap().to_string(),
            metadata: match serde_json::from_str(row.get("metadata").unwrap()) {
                Ok(m) => m,
                Err(_) => return Err(StrawError::ValueError),
            },
            skills: match serde_json::from_str(row.get("skills").unwrap()) {
                Ok(m) => m,
                Err(_) => return Err(StrawError::ValueError),
            },
            joined: row.get("joined").unwrap().parse::<u128>().unwrap(),
        };

        self.base
            .cachedb
            .set(
                format!("sr_profile:{}", username),
                serde_json::to_string::<Profile>(&user).unwrap(),
            )
            .await;

        // return
        Ok(user)
    }

    // SET
    /// Create a new user given their username. Returns their hashed ID
    ///
    /// # Arguments:
    /// * `username` - `String` of the user's `username`
    pub async fn create_profile(&self, username: String) -> Result<String> {
        // make sure user doesn't already exists
        if let Ok(_) = &self.get_profile_by_username(username.clone()).await {
            return Err(StrawError::MustBeUnique);
        };

        // check username
        let regex = regex::RegexBuilder::new("^[\\w\\_\\-\\.\\!]+$")
            .multi_line(true)
            .build()
            .unwrap();

        if regex.captures(&username).iter().len() < 1 {
            return Err(StrawError::ValueError);
        }

        if (username.len() < 2) | (username.len() > 500) {
            return Err(StrawError::ValueError);
        }

        // ...
        let query: &str = if (self.base.db._type == "sqlite") | (self.base.db._type == "mysql") {
            "INSERT INTO \"sr_profiles\" VALUES (?, ?, ?, ?, ?)"
        } else {
            "INSERT INTO \"sr_profiles\" VALUES ($1, $2, $3, $4, $5)"
        };

        let user_id_unhashed: String = dorsal::utility::uuid();
        let user_id_hashed: String = dorsal::utility::hash(user_id_unhashed.clone());
        let timestamp = utility::unix_epoch_timestamp().to_string();

        let c = &self.base.db.client;
        match sqlquery(query)
            .bind::<&String>(&user_id_hashed)
            .bind::<&String>(&username.to_lowercase())
            .bind::<&String>(
                &serde_json::to_string::<ProfileMetadata>(&ProfileMetadata {
                    secondary_token: String::new(),
                })
                .unwrap(),
            )
            .bind::<&String>(&timestamp)
            .bind::<&String>(
                &serde_json::to_string::<Vec<Skill>>(&[SkillName::Normal.into()].to_vec()).unwrap(),
            )
            .execute(c)
            .await
        {
            Ok(_) => Ok(user_id_unhashed),
            Err(_) => Err(StrawError::Other),
        }
    }

    /// Update a [`Profile`]'s metadata by its `username`
    pub async fn edit_profile_metadata_by_name(
        &self,
        name: String,
        metadata: ProfileMetadata,
    ) -> Result<()> {
        // make sure user exists
        if let Err(e) = self.get_profile_by_username(name.clone()).await {
            return Err(e);
        };

        // update user
        let query: &str = if (self.base.db._type == "sqlite") | (self.base.db._type == "mysql") {
            "UPDATE \"sr_profiles\" SET \"metadata\" = ? WHERE \"username\" = ?"
        } else {
            "UPDATE \"sr_profiles\" SET (\"metadata\") = ($1) WHERE \"username\" = $2"
        };

        let c = &self.base.db.client;
        let meta = &serde_json::to_string(&metadata).unwrap();
        match sqlquery(query)
            .bind::<&String>(meta)
            .bind::<&String>(&name)
            .execute(c)
            .await
        {
            Ok(_) => {
                self.base
                    .cachedb
                    .remove(format!("sr_profile:{}", name))
                    .await;
                Ok(())
            }
            Err(_) => Err(StrawError::Other),
        }
    }

    /// Update a [`Profile`]'s skills by its `username`
    pub async fn edit_profile_skills_by_name(&self, name: String, skills: SkillSet) -> Result<()> {
        // make sure user exists
        if let Err(e) = self.get_profile_by_username(name.clone()).await {
            return Err(e);
        };

        // update user
        let query: &str = if (self.base.db._type == "sqlite") | (self.base.db._type == "mysql") {
            "UPDATE \"sr_profiles\" SET \"skills\" = ? WHERE \"username\" = ?"
        } else {
            "UPDATE \"sr_profiles\" SET (\"skills\") = ($1) WHERE \"username\" = $2"
        };

        let c = &self.base.db.client;
        let skills = &serde_json::to_string(&skills).unwrap();
        match sqlquery(query)
            .bind::<&String>(skills)
            .bind::<&String>(&name)
            .execute(c)
            .await
        {
            Ok(_) => {
                self.base
                    .cachedb
                    .remove(format!("sr_profile:{}", name))
                    .await;
                Ok(())
            }
            Err(_) => Err(StrawError::Other),
        }
    }
}
