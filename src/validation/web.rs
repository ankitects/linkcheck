use crate::validation::{CacheEntry, Context, Reason};
use http::HeaderMap;
use kuchiki::parse_html;
use kuchiki::traits::TendrilSink;
use percent_encoding::percent_decode;
use reqwest::{Client, Response, Url};
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

    if already_valid(&url, ctx) {
        log::debug!("The cache says \"{}\" is still valid", url);
        return Ok(());
    }

    let result = if let Some(fragment) = url.fragment() {
        let fragment = percent_decode(fragment.as_bytes()).decode_utf8_lossy();
        log::debug!("Checking \"{}\" contains \"{}\"", url, fragment);
        let response =
            get(ctx.client(), url.clone(), ctx.url_specific_headers(&url))
                .await?;
        let document = parse_html()
            .from_utf8()
            .read_from(&mut response.text().await?.as_bytes())?;
        document
            .select_first(&format!("#{}", fragment))
            .map(|_| ())
            .map_err(|_| Reason::Dom)
    } else {
        head(ctx.client(), url.clone(), ctx.url_specific_headers(&url))
            .await
            .map_err(Reason::from)
    };

    let entry = CacheEntry::new(SystemTime::now(), result.is_ok());
    update_cache(url, ctx, entry);

    result
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
