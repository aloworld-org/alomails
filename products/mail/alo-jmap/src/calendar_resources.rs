//! Rooms and resources — the admin surface that creates them and the list
//! everyone reads to book one.
//!
//! - `GET    /calendar/resources` — every room in the workspace. Any member:
//!   you cannot pick a room you cannot see.
//! - `POST   /calendar/resources` — add one (admin).
//! - `PUT    /calendar/resources/{id}` — rename / re-address / re-measure it
//!   (admin).
//! - `DELETE /calendar/resources/{id}` — retire it (admin). Meetings that held
//!   it keep their time and lose the room.
//!
//! Booking is not here: an event books a room by naming its address among its
//! attendees, and [`crate::calendar`] holds it at save time. Whether a room is
//! free is answered by `POST /calendar/freebusy` for a room's address exactly
//! as for a person's.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{CalendarId, CalendarResource};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// `GET /calendar/resources` → `{"resources": [...]}`, by name.
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let resources = account
        .acc
        .calendar_resources()
        .await
        .map_err(|_| Problem::server_error())?;
    let out: Vec<Value> = resources.iter().map(resource_json).collect();
    Ok(Json(json!({ "resources": out })))
}

#[derive(Deserialize)]
struct ResourceBody {
    /// Display name ("Board room").
    name: String,
    /// The address meetings name it by.
    email: String,
    /// Where it is, or absent.
    #[serde(default)]
    location: Option<String>,
    /// How many people it seats, or absent.
    #[serde(default)]
    capacity: Option<i32>,
}

impl ResourceBody {
    /// The store's shape. The id is a placeholder on the way in — creating
    /// mints one, updating addresses the row by its path segment.
    fn into_resource(self) -> CalendarResource {
        CalendarResource {
            id: CalendarId::new(String::new()),
            name: self.name,
            email: self.email,
            location: self.location.filter(|l| !l.trim().is_empty()),
            capacity: self.capacity,
        }
    }
}

/// `POST /calendar/resources` → the created resource. Admin only.
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let req: ResourceBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let resource = req.into_resource();
    // The store re-checks the invariants and owns the taken-address refusal;
    // its message is the verbatim, actionable one (422 / 409).
    let id = account
        .acc
        .create_calendar_resource(&resource)
        .await
        .map_err(Problem::from)?;
    Ok(Json(resource_json(&CalendarResource { id, ..resource })))
}

/// `PUT /calendar/resources/{id}` → the updated resource. Admin only.
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let req: ResourceBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let id = CalendarId::new(id);
    let resource = req.into_resource();
    account
        .acc
        .update_calendar_resource(&id, &resource)
        .await
        .map_err(Problem::from)?;
    Ok(Json(resource_json(&CalendarResource { id, ..resource })))
}

/// `DELETE /calendar/resources/{id}`. Admin only.
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    account
        .acc
        .delete_calendar_resource(&CalendarId::new(id))
        .await
        .map_err(Problem::from)?;
    Ok(Json(json!({ "status": "ok" })))
}

/// A resource as the wire serves it.
pub(crate) fn resource_json(resource: &CalendarResource) -> Value {
    json!({
        "id": resource.id.as_str(),
        "name": resource.name,
        "email": resource.email,
        "location": resource.location,
        "capacity": resource.capacity,
    })
}
