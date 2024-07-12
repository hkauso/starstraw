use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::database::Result;
use dorsal::DefaultReturn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SkillType {
    /// Modifies the `defense` level of a profile
    ///
    /// Modification amount is initial modification amount * skill level * title skill level
    ModifierD,
    /// Modifies the `power` level of a profile
    ///
    /// Modification amount is initial modification amount * skill level * title skill level
    ModifierP,
    /// An action that can be cast by a profile
    Ability,
    /// A profile role that multiplies (or divides) the level of **every** skill
    ///
    /// The title skill level changes the skill level multiplication amount.
    Title,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SkillName {
    // modifiers
    /// `ModifierP` type skill; *2 power values
    Master,
    /// `ModifierD` type skill; *2 defensive values
    Patron,
    /// `ModifierP` type skill; *1.05 power values
    Trustworthy,
    /// `ModifierD` type skill; *1.05 defensive values
    Protected,
    // abilities
    /// `Ability` type skill; the ability to do anything and everything; should be
    /// ignored if the user has a power level of less than 100,000
    Absolute,
    // titles
    /// `Title` type skill; multiplies all skill levels by 100,000; allows user to edit
    /// the skills of other users
    God,
    /// `Title` type skill; multiplies all skill levels by 10,000
    Administrator,
    /// `Title` type skill; multiplies all skill levels by 1,000
    Manager,
    /// `Title` type skill; multiplies all skill levels by 1
    Normal,
}

impl Into<Skill> for SkillName {
    /// Get a skill and its values from just its name
    fn into(self) -> Skill {
        use SkillName::*;
        match self {
            // modifiers
            Master => ((SkillType::ModifierP, self), 2.0),
            Patron => ((SkillType::ModifierD, self), 2.0),
            Trustworthy => ((SkillType::ModifierP, self), 1.05),
            Protected => ((SkillType::ModifierD, self), 1.05),
            // abilities
            Absolute => ((SkillType::Ability, self), 1.0),
            // titles
            God => ((SkillType::Title, self), 100_000.0),
            Administrator => ((SkillType::Title, self), 10_000.0),
            Manager => ((SkillType::Title, self), 1_000.0),
            Normal => ((SkillType::Title, self), 1.0),
        }
    }
}

impl SkillName {
    /// Check if a skill is valid based on other skills or the user's overall levels
    pub fn is_valid(&self, stats: ProfileStats) -> bool {
        // check if the skill is valid
        if (self == &SkillName::Absolute) && (stats.power < 100_000.0) {
            // we must have a power level of at least 100,000 to get absolute power
            return false;
        } else if self == &SkillName::God {
            // "God" title cannot be granted at all
            return false;
        }

        // by default it's valid!
        true
    }
}

/// A basic skill - the `f32` skill number is usually the default value,
/// but it can be set to something else when the skill is granted if the skill
/// is a different level than its default value (default * level)
pub type Skill = (SkillIdentifier, f32);
pub type SkillSet = Vec<Skill>;
/// Only what's needed to identify a skill
pub type SkillIdentifier = (SkillType, SkillName);

/// Basic user structure
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Profile {
    pub id: String,
    pub username: String,
    pub metadata: ProfileMetadata,
    pub skills: SkillSet,
    pub joined: u128,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProfileMetadata {
    /// A secondary token that can be used to authenticate as the account
    #[serde(default)]
    pub secondary_token: String,
}

// props
#[derive(Serialize, Deserialize, Debug)]
pub struct ProfileCreate {
    pub username: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProfileLogin {
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GrantSkill {
    pub skill: Skill,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RevokeSkill {
    pub skill: SkillName,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GrantTitle {
    pub title: SkillName,
}

/// General API errors
pub enum StrawError {
    MustBeUnique,
    NotAllowed,
    ValueError,
    NotFound,
    Other,
}

impl StrawError {
    pub fn to_string(&self) -> String {
        use StrawError::*;
        match self {
            MustBeUnique => String::from("One of the given values must be unique."),
            NotAllowed => String::from("You are not allowed to access this resource."),
            ValueError => String::from("One of the field values given is invalid."),
            NotFound => String::from("No asset with this ID could be found."),
            _ => String::from("An unspecified error has occured"),
        }
    }
}

impl IntoResponse for StrawError {
    fn into_response(self) -> Response {
        use crate::model::StrawError::*;
        match self {
            NotAllowed => (
                StatusCode::UNAUTHORIZED,
                Json(DefaultReturn::<u16> {
                    success: false,
                    message: self.to_string(),
                    payload: 401,
                }),
            )
                .into_response(),
            NotFound => (
                StatusCode::NOT_FOUND,
                Json(DefaultReturn::<u16> {
                    success: false,
                    message: self.to_string(),
                    payload: 404,
                }),
            )
                .into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DefaultReturn::<u16> {
                    success: false,
                    message: self.to_string(),
                    payload: 500,
                }),
            )
                .into_response(),
        }
    }
}

// ...
/// Simple manager for profile skills
#[derive(Clone)]
pub struct SkillManager(pub SkillSet);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProfileStats {
    pub power: f32,
    pub defense: f32,
    pub title: SkillName,
    pub abilities: HashMap<SkillName, f32>,
    pub skills: SkillSet,
}

impl Default for ProfileStats {
    fn default() -> Self {
        Self {
            power: 1.0,
            defense: 1.0,
            title: SkillName::Normal,
            abilities: HashMap::new(),
            skills: [SkillName::Normal.into()].to_vec(),
        }
    }
}

impl SkillManager {
    /// Get profile statistics based on its skills
    pub fn get_stats(&self) -> ProfileStats {
        let mut iter = self.0.iter();

        // resolve title
        // our title is the first title present
        let title = iter
            .find(|s| s.0 .0 == SkillType::Title)
            // if we couldn't find the title, use whatever
            .unwrap_or(&((SkillType::Title, SkillName::Normal), 0.0))
            .clone();

        // resolve power, defense, and abilities
        let mut power: f32 = 1.0;
        let mut defense: f32 = 1.0;
        let mut abilities = HashMap::new();

        for skill in iter {
            // we're only resolving things that modify our values here, so we'll just do what they say
            match skill.0 .0 {
                SkillType::ModifierD => defense *= skill.1,
                SkillType::ModifierP => power *= skill.1,
                SkillType::Ability => {
                    abilities.insert(skill.0 .1.clone(), skill.1);
                    ()
                }
                _ => continue,
            }
        }

        // use title
        power *= title.1;
        defense *= title.1;

        // return
        ProfileStats {
            power,
            defense,
            title: title.0 .1,
            abilities,
            skills: self.0.clone(),
        }
    }

    /// Update the profile title
    pub fn title(&mut self, skill: Skill) -> Result<()> {
        // find current title location
        for (i, skill) in self.0.clone().iter().enumerate() {
            if skill.0 .0 != SkillType::Title {
                continue;
            }

            let _ = std::mem::replace(&mut self.0[i], skill.to_owned());
            return Ok(());
        }

        // since we didn't return earlier, we didn't previously have a title skill
        // this means we can just insert the skill at 0
        self.0.insert(0, skill);
        Ok(())
    }

    /// Remove the given skill by name
    pub fn remove(&mut self, name: SkillName) -> Result<()> {
        for (i, skill) in self.0.clone().iter().enumerate() {
            if skill.0 .1 != name {
                continue;
            }

            self.0.remove(i);
        }

        Ok(())
    }

    /// Push the given skill
    pub fn push(&mut self, skill: Skill) -> Result<()> {
        // make sure skill is valid
        // this makes sure we aren't granted any skills we shouldn't be able to have
        if !skill.0 .1.is_valid(self.get_stats()) {
            return Err(StrawError::ValueError);
        }

        // ...
        self.0.push(skill);
        Ok(())
    }

    /// Check if the profile is allowed to act on another [`SkillManager`] by
    /// comparing their stats
    pub fn act(&self, other: SkillManager) -> bool {
        let me = self.get_stats();
        let them = other.get_stats();
        ((me.power > them.defense) && (them.power <= me.power)) | (me.title == SkillName::God)
    }
}
