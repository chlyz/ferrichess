use crate::AppResult;

const MAX_RESPONSE_BYTES: u64 = 1_000_000_000;

pub fn get_text(url: &str, accept: &str) -> AppResult<String> {
    get_optional_text_with_query(url, accept, &[], None)?
        .ok_or_else(|| format!("resource not found: {url}").into())
}

pub fn get_optional_text_with_query(
    url: &str,
    accept: &str,
    query: &[(&str, &str)],
    bearer_token: Option<&str>,
) -> AppResult<Option<String>> {
    let mut request = ureq::get(url)
        .query_pairs(query.iter().copied())
        .header("Accept", accept)
        .header(
            "User-Agent",
            "ferrichess-archive/0.1 (public personal archive tool)",
        );
    if let Some(token) = bearer_token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let mut response = match request.call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    Ok(Some(
        response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()?,
    ))
}
