use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct XataBranch {
    pub id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    #[serde(rename = "connectionString")]
    pub connection_string: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct XataBranchesResponse {
    pub branches: Vec<XataBranch>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateBranchRequest {
    pub mode: String,
    pub name: String,
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum XataError {
    #[error("Xata API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("HTTP request failed: {0}")]
    Http(Box<ureq::Error>),

    #[error("Failed to parse response: {0}")]
    Parse(String),
}

impl From<ureq::Error> for XataError {
    fn from(err: ureq::Error) -> Self {
        XataError::Http(Box::new(err))
    }
}

impl XataError {
    pub fn is_not_found(&self) -> bool {
        match self {
            XataError::Api { status: 404, .. } => true,
            XataError::Http(err) => matches!(**err, ureq::Error::Status(404, _)),
            _ => false,
        }
    }
}
pub struct XataClient {
    base_url: String,
    api_key: String,
    org: String,
    project: String,
    client: ureq::Agent,
}

impl XataClient {
    pub fn new(config: &crate::config::ResolvedConfig) -> Self {
        Self {
            base_url: "https://api.xata.tech".to_string(),
            api_key: config.api_key.clone(),
            org: config.org.clone(),
            project: config.project.clone(),
            client: ureq::Agent::new(),
        }
    }

    /// Builder method to override the base URL (useful for mock testing)
    #[cfg(test)]
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    fn client(&self) -> &ureq::Agent {
        &self.client
    }

    fn handle_error_response(&self, response: ureq::Response) -> XataError {
        let status = response.status();
        let text = response.into_string().unwrap_or_default();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(msg) = val.get("message").and_then(|m| m.as_str())
        {
            return XataError::Api {
                status,
                message: msg.to_string(),
            };
        }
        XataError::Api {
            status,
            message: text,
        }
    }

    /// Retrieves detailed information about a specific branch.
    /// Returns Ok(Some(branch)) if found, Ok(None) if 404 (missing), or Err on failure.
    pub fn get_branch(&self, branch_name: &str) -> Result<Option<XataBranch>, XataError> {
        let target_id = if let Ok(branches) = self.list_branches() {
            let Some(b) = branches
                .iter()
                .find(|b| b.name == branch_name || b.id == branch_name)
            else {
                return Ok(None);
            };
            b.id.clone()
        } else {
            branch_name.to_string()
        };

        let url = format!(
            "{}/organizations/{}/projects/{}/branches/{}",
            self.base_url, self.org, self.project, target_id
        );

        let response = match self
            .client()
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .call()
        {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_code, resp)) => resp,
            Err(err) => return Err(err.into()),
        };

        if response.status() == 404 {
            return Ok(None);
        }

        if response.status() < 200 || response.status() >= 300 {
            return Err(self.handle_error_response(response));
        }

        let branch: XataBranch = response
            .into_json()
            .map_err(|e| XataError::Parse(e.to_string()))?;

        Ok(Some(branch))
    }

    /// Creates a new branch, optionally inheriting from a parent branch.
    pub fn create_branch(
        &self,
        branch_name: &str,
        parent_branch: Option<&str>,
    ) -> Result<XataBranch, XataError> {
        let url = format!(
            "{}/organizations/{}/projects/{}/branches",
            self.base_url, self.org, self.project
        );

        let payload = CreateBranchRequest {
            mode: "inherit".to_string(),
            name: branch_name.to_string(),
            parent_id: parent_branch.map(|s| s.to_string()),
        };

        let response = match self
            .client()
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(&payload)
        {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_code, resp)) => resp,
            Err(err) => return Err(err.into()),
        };

        if response.status() < 200 || response.status() >= 300 {
            return Err(self.handle_error_response(response));
        }

        let branch: XataBranch = response
            .into_json()
            .map_err(|e| XataError::Parse(e.to_string()))?;

        Ok(branch)
    }

    /// Permanently deletes a specific branch.
    pub fn delete_branch(&self, branch_name: &str) -> Result<(), XataError> {
        let target_id = if let Ok(branches) = self.list_branches() {
            let Some(b) = branches
                .iter()
                .find(|b| b.name == branch_name || b.id == branch_name)
            else {
                return Ok(());
            };
            b.id.clone()
        } else {
            branch_name.to_string()
        };

        let url = format!(
            "{}/organizations/{}/projects/{}/branches/{}",
            self.base_url, self.org, self.project, target_id
        );

        let response = match self
            .client()
            .delete(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .call()
        {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_code, resp)) => resp,
            Err(err) => return Err(err.into()),
        };

        if response.status() == 404 {
            return Ok(());
        }

        if response.status() < 200 || response.status() >= 300 {
            return Err(self.handle_error_response(response));
        }

        Ok(())
    }

    /// Retrieves all branches for the active project.
    pub fn list_branches(&self) -> Result<Vec<XataBranch>, XataError> {
        let url = format!(
            "{}/organizations/{}/projects/{}/branches",
            self.base_url, self.org, self.project
        );

        let response = match self
            .client()
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .call()
        {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_code, resp)) => resp,
            Err(err) => return Err(err.into()),
        };

        if response.status() < 200 || response.status() >= 300 {
            return Err(self.handle_error_response(response));
        }

        let res: XataBranchesResponse = response
            .into_json()
            .map_err(|e| XataError::Parse(e.to_string()))?;

        Ok(res.branches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn test_config() -> crate::config::ResolvedConfig {
        crate::config::ResolvedConfig {
            org: "test-org".to_string(),
            project: "test-proj".to_string(),
            database: "test-db".to_string(),
            fallback_parent: "main".to_string(),
            api_key: "test-key".to_string(),
            post_create: None,
            auto_prune: true,
        }
    }

    #[test]
    fn test_get_branch_success() {
        let mut server = Server::new();
        let mock = server
            .mock(
                "GET",
                "/organizations/test-org/projects/test-proj/branches/my-branch",
            )
            .match_header("Authorization", "Bearer test-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "id": "my-branch",
                "name": "my-branch",
                "createdAt": "2023-11-07T05:31:56Z",
                "parentID": "main",
                "connectionString": "postgresql://test"
            }"#,
            )
            .create();

        let client = XataClient::new(&test_config()).with_base_url(server.url());
        let res = client.get_branch("my-branch").unwrap();

        assert_eq!(
            res,
            Some(XataBranch {
                id: "my-branch".to_string(),
                name: "my-branch".to_string(),
                created_at: Some("2023-11-07T05:31:56Z".to_string()),
                parent_id: Some("main".to_string()),
                connection_string: Some("postgresql://test".to_string()),
            })
        );
        mock.assert();
    }

    #[test]
    fn test_get_branch_not_found() {
        let mut server = Server::new();
        let mock = server
            .mock(
                "GET",
                "/organizations/test-org/projects/test-proj/branches/missing-branch",
            )
            .with_status(404)
            .create();

        let client = XataClient::new(&test_config()).with_base_url(server.url());
        let res = client.get_branch("missing-branch").unwrap();

        assert_eq!(res, None);
        mock.assert();
    }

    #[test]
    fn test_create_branch_success() {
        let mut server = Server::new();
        let mock = server
            .mock(
                "POST",
                "/organizations/test-org/projects/test-proj/branches",
            )
            .match_header("Authorization", "Bearer test-key")
            .match_body(mockito::Matcher::JsonString(
                r#"{"mode":"inherit","name":"new-branch","parentID":"main"}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "id": "new-branch",
                "name": "new-branch",
                "parentID": "main"
            }"#,
            )
            .create();

        let client = XataClient::new(&test_config()).with_base_url(server.url());
        let res = client.create_branch("new-branch", Some("main")).unwrap();

        assert_eq!(
            res,
            XataBranch {
                id: "new-branch".to_string(),
                name: "new-branch".to_string(),
                created_at: None,
                parent_id: Some("main".to_string()),
                connection_string: None,
            }
        );
        mock.assert();
    }
}
