use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::db::Pool;
use crate::errors::AppError;
use crate::models::server_news::ServerNews;
use crate::services::server_news_service;

#[derive(Debug, Deserialize)]
pub struct ListNewsQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct PlanetNewsListItem {
    pub id: Uuid,
    pub title: String,
    pub summary: Option<String>,
    pub markdown_content: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Serialize)]
pub struct PlanetNewsDetail {
    pub id: Uuid,
    pub title: String,
    pub summary: Option<String>,
    pub markdown_content: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn to_list_item(item: ServerNews) -> PlanetNewsListItem {
    PlanetNewsListItem {
        id: item.id,
        title: item.title,
        summary: item.summary,
        markdown_content: item.markdown_content,
        published_at: item.published_at,
    }
}

fn to_detail(item: ServerNews) -> PlanetNewsDetail {
    PlanetNewsDetail {
        id: item.id,
        title: item.title,
        summary: item.summary,
        markdown_content: item.markdown_content,
        published_at: item.published_at,
        updated_at: item.updated_at,
    }
}

pub async fn list_news(
    pool: web::Data<Pool>,
    _auth: AuthUser,
    query: web::Query<ListNewsQuery>,
) -> Result<HttpResponse, AppError> {
    let items = server_news_service::list_news(&pool, query.limit.unwrap_or(30))?;
    Ok(HttpResponse::Ok().json(items.into_iter().map(to_list_item).collect::<Vec<_>>()))
}

pub async fn get_news_detail(
    pool: web::Data<Pool>,
    _auth: AuthUser,
    news_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let item = server_news_service::get_news_by_id(&pool, *news_id)?.ok_or(AppError::NotFound)?;
    Ok(HttpResponse::Ok().json(to_detail(item)))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("", web::get().to(list_news))
        .route("/{news_id}", web::get().to(get_news_detail));
}
