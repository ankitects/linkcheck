use crate::validation::{CacheEntry, Context, Reason};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use http::HeaderMap;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use reqwest::{Client, Response, Url};
use std::borrow::Borrow;
use std::time::SystemTime;

/// Send a GET request to a particular endpoint.
pub async fn get(
    client: &Client,
    url: Url,
    extra_headers: HeaderMap,
) -> Result<Response, reqwest::Error> {
    client
        .get(url)
        .headers(extra_headers)
        .send()
        .await?
        .error_for_status()
}

/// Send a HEAD request to a particular endpoint.
pub async fn head(
    client: &Client,
    url: Url,
    extra_headers: HeaderMap,
) -> Result<(), reqwest::Error> {
    client
        .head(url)
        .headers(extra_headers)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}

/// Check whether a [`Url`] points to a valid resource on the internet.
pub async fn check_web<C>(url: &Url, ctx: &C) -> Result<(), Reason>
where
    C: Context + ?Sized,
{
    log::debug!("Checking \"{}\" on the web", url);

    if already_valid(url, ctx) {
        log::debug!("The cache says \"{}\" is still valid", url);
        return Ok(());
    }

    let result = if let Some(fragment) = url.fragment() {
        log::debug!("Checking \"{}\" contains \"{}\"", url, fragment);
        let response =
            get(ctx.client(), url.clone(), ctx.url_specific_headers(url))
                .await?;
        if element_with_id_exists(response.text().await?.as_bytes(), fragment) {
            Ok(())
        } else {
            Err(Reason::Dom)
        }
    } else {
        head(ctx.client(), url.clone(), ctx.url_specific_headers(url))
            .await
            .map_err(Reason::from)
    };

    let entry = CacheEntry::new(SystemTime::now(), result.is_ok());
    update_cache(url, ctx, entry);

    result
}

fn element_with_id_exists(mut html: &[u8], id: &str) -> bool {
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html)
        .unwrap();
    node_has_element_with_id(&dom.document, id)
}

fn node_has_element_with_id(node: &Handle, id: &str) -> bool {
    let node_data = node.data.borrow();
    if let NodeData::Element { ref attrs, .. } = *node_data {
        if attrs
            .borrow()
            .iter()
            .any(|a| a.name.local.as_ref() == "id" && a.value.as_ref() == id)
        {
            return true;
        }
    }
    node.children
        .borrow()
        .iter()
        .any(|child| node_has_element_with_id(child, id))
}

fn already_valid<C>(url: &Url, ctx: &C) -> bool
where
    C: Context + ?Sized,
{
    if let Some(cache) = ctx.cache() {
        return cache.url_is_still_valid(url, ctx.cache_timeout());
    }

    false
}

fn update_cache<C>(url: &Url, ctx: &C, entry: CacheEntry)
where
    C: Context + ?Sized,
{
    if let Some(mut cache) = ctx.cache() {
        cache.insert(url.clone(), entry);
    }
}
