use crate::validation::{CacheEntry, Context, Reason};
use html5ever::{parse_document, tendril::TendrilSink};
use http::HeaderMap;
use markup5ever_rcdom::{NodeData, RcDom};
use reqwest::{Client, Response, Url};
use std::{borrow::Borrow, time::SystemTime};

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

    match url.fragment() {
        Some(fragment) => {
            log::debug!("Checking \"{}\" contains \"{}\"", url, fragment);
            check_fragment_url(url, fragment, ctx).await
        },
        None => {
            let result =
                head(ctx.client(), url.clone(), ctx.url_specific_headers(url))
                    .await
                    .map_err(Reason::from);
            update_cache(url, ctx, result.is_ok());
            result
        },
    }
}

async fn check_fragment_url(
    url: &Url,
    fragment: &str,
    ctx: &(impl Context + ?Sized),
) -> Result<(), Reason> {
    let response =
        get(ctx.client(), url.clone(), ctx.url_specific_headers(url)).await?;
    cache_url_fragment(ctx, url, None);

    let mut found = false;
    walk_element_ids(response.text().await?.as_bytes(), |id: &str| {
        cache_url_fragment(ctx, url, Some(id));
        found |= id == fragment;
        // if caching, process all ids
        found && ctx.cache().is_none()
    });

    if found {
        Ok(())
    } else {
        Err(Reason::Dom)
    }
}

fn cache_url_fragment(
    ctx: &(impl Context + ?Sized),
    url: &Url,
    fragment: Option<&str>,
) {
    if let Some(mut cache) = ctx.cache() {
        let mut fragment_url = url.clone();
        fragment_url.set_fragment(fragment);
        let entry = CacheEntry::new(SystemTime::now(), true);
        cache.insert(fragment_url, entry);
    }
}

/// Walk element ids until `processor` returns true.
fn walk_element_ids(mut html: &[u8], mut processor: impl FnMut(&str) -> bool) {
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html)
        .unwrap();
    let mut stack = vec![dom.document];
    while let Some(node) = stack.pop() {
        if let NodeData::Element { ref attrs, .. } = *node.data.borrow() {
            for attr in attrs.borrow().iter() {
                if attr.name.local.as_ref() == "id"
                    && processor(attr.value.as_ref())
                {
                    return;
                }
            }
        }
        stack.extend(std::mem::take(&mut *node.children.borrow_mut()));
    }
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

fn update_cache<C>(url: &Url, ctx: &C, valid: bool)
where
    C: Context + ?Sized,
{
    if let Some(mut cache) = ctx.cache() {
        let entry = CacheEntry::new(SystemTime::now(), valid);
        cache.insert(url.clone(), entry);
    }
}
