use crate::{app_config::AppConfig, connect::error::ConnectorError};
use reqwest::Response;
use serde_json::{from_str, Value};

static VALIDATION_URL: &str = "https://id.twitch.tv/oauth2/validate";
static REDIRECT_URI: &str = "https://localhost:3030";

fn get_from_json<'a, 'b>(val: &'a Value, key: &'b str) -> Result<&'a str, ConnectorError> {
    val[key].as_str().ok_or_else(|| {
        ConnectorError::MessageReceiveFailed(format!("No field named '{}' in JSON {:?}", key, val,))
    })
}

async fn get_json_from_response(response: Response) -> Result<Value, ConnectorError> {
    let response_text = response.text().await.map_err(|err| {
        ConnectorError::MessageReceiveFailed(format!("Could not get response text: {:?}", err))
    })?;
    let val: Value = from_str(&response_text).map_err(|err| {
        ConnectorError::MessageReceiveFailed(format!("Could not parse response json: {:?}", err))
    })?;
    Ok(val)
}

fn extract_code_from_url(request_url: &str) -> &str {
    let code_start = request_url.find("code=").unwrap() + 5;
    let code_end = code_start + 30;
    &request_url[code_start..code_end]
}

pub struct AccessTokenDispenser<'a> {
    app_config: &'a AppConfig,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

impl<'a> AccessTokenDispenser<'a> {
    pub fn new(app_config: &'a AppConfig) -> Self {
        Self {
            app_config,
            access_token: None,
            refresh_token: None,
        }
    }

    fn retrieve_code(&self) -> Result<String, ConnectorError> {
        let ssl_config = tiny_http::SslConfig {
            certificate: include_bytes!("../../../../certificates/cert.crt").to_vec(),
            // TODO: security: must not include private keys in a binary !
            private_key: include_bytes!("../../../../certificates/cert.key").to_vec(),
        };
        let server = tiny_http::Server::https("0.0.0.0:3030", ssl_config).map_err(|err| {
            ConnectorError::MessageReceiveFailed(format!(
                "Could not start server to get code: {:?}",
                err
            ))
        })?;
        println!(
            "Open link https://id.twitch.tv/oauth2/authorize?client_id={}&redirect_uri=https://localhost:3030&response_type=code&scope=chat:read%20chat:edit",
            self.app_config.twitch_client_id(),
        );
        let request: tiny_http::Request = server.recv().map_err(|err| {
            ConnectorError::MessageReceiveFailed(format!(
                "Could not receive request with code: {:?}",
                err
            ))
        })?;
        let request_url = request.url();
        let code = extract_code_from_url(request_url);
        Ok(code.to_owned())
    }

    async fn access_token_is_valid(&self) -> Result<bool, ConnectorError> {
        if self.access_token.is_none() {
            return Ok(false);
        }
        let client = reqwest::Client::new();
        let access_token = self.access_token.as_ref().unwrap();
        let validation_response = client
            .get(VALIDATION_URL)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|err| {
                ConnectorError::MessageReceiveFailed(format!(
                    "Could not validate access token: {:?}",
                    err
                ))
            })?;
        if validation_response.status() != 200 {
            let json = get_json_from_response(validation_response).await?;
            let message = get_from_json(&json, "message")?;
            return Err(ConnectorError::MessageReceiveFailed(format!(
                "Access token invalid: {}",
                message
            )));
        }
        Ok(true)
    }

    async fn renew(&mut self) -> Result<(), ConnectorError> {
        let code = self.retrieve_code()?;
        let uri = format!(
            "https://id.twitch.tv/oauth2/token?client_id={}&client_secret={}&code={}&grant_type=authorization_code&redirect_uri={}",
            self.app_config.twitch_client_id(),
            self.app_config.twitch_client_secret(),
            code,
            REDIRECT_URI,
        );
        let client = reqwest::Client::new();
        let response = client.post(uri).send().await.map_err(|err| {
            ConnectorError::MessageReceiveFailed(format!(
                "Failed to get access token renewal response: {:?}",
                err
            ))
        })?;
        let status = response.status();
        let json = get_json_from_response(response).await?;
        if status != 200 {
            let message = get_from_json(&json, "message")?;
            return Err(ConnectorError::MessageReceiveFailed(format!(
                "Could not get access token: {}",
                message,
            )));
        }
        let access_token = get_from_json(&json, "access_token")?;
        let refresh_token = get_from_json(&json, "refresh_token")?;
        self.access_token = Some(access_token.to_owned());
        self.refresh_token = Some(refresh_token.to_owned());
        Ok(())
    }

    pub async fn get(&mut self) -> Result<&str, ConnectorError> {
        if !self.access_token_is_valid().await? {
            self.renew().await?;
        }
        Ok(self
            .access_token
            .as_ref()
            .expect("Unexpectedly the access token is not set after renewal!"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getting_code_out_of_url() {
        let url1 = "https://localhost:3030/?code=y6gdw1g6vfi07y1otcpn5abfzb94n9&scope=chat%3Aread+chat%3Aedit";
        let url2 = "/?code=y6gdw1g6vfi07y1otcpn5abfzb94n9&scope=chat%3Aread+chat%3Aedit";
        let code = "y6gdw1g6vfi07y1otcpn5abfzb94n9";
        assert_eq!(extract_code_from_url(url1), code);
        assert_eq!(extract_code_from_url(url2), code);
    }
}
