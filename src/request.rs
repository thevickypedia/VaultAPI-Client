use crate::decipher;
use crate::parser::Config;
use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::process::exit;


struct RequestMaterials {
    url: String,
    params: HashMap<String, String>,
    headers: HashMap<String, String>,
}


/// Constructs authentication headers.
///
/// # Arguments
/// * `apikey` - APIkey to authenticate the server.
///
/// # Returns
/// * A `HashMap<String, String>` containing auth headers.
fn auth_headers(apikey: &String) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    let bearer = format!("Bearer {}", apikey);
    headers.insert("Authorization".to_string(), bearer);
    headers.insert("Accept".to_string(), "application/json".to_string());
    headers
}


/// Constructs the required fields to make a request.
///
/// # Arguments
/// * `config` - Config object to retrieve environment variables, and command line arguments.
///
/// # Returns
/// * A `RequestMaterials` struct containing auth headers, query parameters, and the request URL.
fn create_request_materials(config: &Config) -> RequestMaterials {
    // Add URL parameters
    let mut url = String::new();
    let mut params = HashMap::new();

    if !config.table_name.is_empty() {
        params.insert("table_name".to_string(), config.table_name.to_string());
    } else if !config.get_table.is_empty() {
        params.insert("table_name".to_string(), config.get_table.to_string());
    } else {
        println!("Table name is mandatory to retrieve the secret");
        exit(1)
    }

    if !config.get_secrets.is_empty() {
        url = format!("{}get-secrets", &config.vault_server);
        params.insert("keys".to_string(), config.get_secrets.to_string());
    } else if !config.get_secret.is_empty() {
        url = format!("{}get-secret", &config.vault_server);
        params.insert("key".to_string(), config.get_secret.to_string());
    } else if !config.get_table.is_empty() {
        url = format!("{}get-table", &config.vault_server);
    } else if config.table_name.is_empty() {
        println!("Required parameters unfilled!");
        exit(1)
    }
    RequestMaterials {
        url,
        params,
        headers: auth_headers(&config.apikey),
    }
}

/// Function to create a server request and process the response.
///
/// # Arguments
/// * `config` - Config object to retrieve environment variables, and command line arguments.
///
/// # Returns
/// * A `Result<Value, String>` containing deciphered content.
pub fn server_connection(config: &Config) -> Result<Value, String> {
    let request = create_request_materials(config);
    let response = make_request(
        &request.url,
        Some(request.headers),
        Some(request.params)
    );
    // Check if the result is the expected "detail" field, or handle accordingly
    match response {
        Value::Null => {
            println!("No 'detail' key found in the response.");
            exit(1)
        }
        Value::String(cipher_text) => {
            return decipher::transit_decrypt(
                &config.apikey,
                &cipher_text,
                config.transit_key_length,
                config.transit_time_bucket,
            )
        }
        Value::Object(obj) => {
            println!("Detail is an object: {:?}", obj);
        }
        _ => {
            println!("Unexpected value returned: {:?}", response);
        }
    }
    exit(1)
}

/// Function to make a `GET` request to the server.
///
/// # Arguments
/// * `server_url` - Server URL.
/// * `headers` - Authentication headers.
/// * `params` - Query parameters.
///
/// # Returns
/// * A `Value` object containing the server response.
pub fn make_request(
    server_url: &str,
    headers: Option<HashMap<String, String>>,
    params: Option<HashMap<String, String>>,
) -> Value {
    // Create a reqwest client
    let client = Client::new();

    // Build the URL with parameters if provided
    let mut url = reqwest::Url::parse(server_url).expect("Invalid URL");
    if let Some(query_params) = params {
        let query: Vec<(String, String)> = query_params.into_iter().collect();
        url.query_pairs_mut().extend_pairs(query);
    }

    // Prepare the request builder
    let mut request = client.get(url);

    // Add headers if provided
    if let Some(custom_headers) = headers {
        for (key, value) in custom_headers {
            request = request.header(&key, value);
        }
    }

    // Make the request
    match request.send() {
        Ok(response) => {
            match response.json::<Value>() {
                Ok(json) => {
                    // Try to get the value of "detail" if it exists
                    if let Some(detail) = json.get("detail") {
                        detail.clone()
                    } else {
                        // Return null if "detail" key is not present
                        Value::Null
                    }
                }
                Err(err) => {
                    println!("Failed to parse response as JSON: {}", err);
                    exit(1);
                }
            }
        }
        Err(err) => {
            println!("Failed to fetch data from {}: {}", server_url, err);
            exit(1);
        }
    }
}
