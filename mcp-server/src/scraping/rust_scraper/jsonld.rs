use super::RustScraper;
use scraper::{Html, Selector};

impl RustScraper {
    /// Extract JSON-LD structured data (Schema.org) from <script type="application/ld+json">
    pub(super) fn extract_json_ld(&self, document: &Html) -> Option<String> {
        let selector = Selector::parse("script[type='application/ld+json']").ok()?;

        let mut structured_content = Vec::new();

        for script in document.select(&selector) {
            let json_text = script.inner_html();
            if json_text.trim().is_empty() {
                continue;
            }

            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&json_text) {
                self.collect_json_ld(&json_value, &mut structured_content);
            }
        }

        if structured_content.is_empty() {
            None
        } else {
            Some(structured_content.join("\n\n---\n\n"))
        }
    }

    fn collect_json_ld(&self, value: &serde_json::Value, structured_content: &mut Vec<String>) {
        match value {
            serde_json::Value::Array(items) => {
                for item in items {
                    self.collect_json_ld(item, structured_content);
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(graph) = map.get("@graph") {
                    self.collect_json_ld(graph, structured_content);
                }

                let type_val = map.get("@type").and_then(|v| v.as_str()).unwrap_or("");
                match type_val {
                    "Product" => {
                        let name = self.json_ld_string(map.get("name"));
                        let description = self.json_ld_string(map.get("description"));
                        let price = self.json_ld_price(map.get("offers"));

                        if let Some(name) = name {
                            let mut entry = format!("# {}", name);
                            if let Some(desc) = description {
                                if !desc.is_empty() {
                                    entry.push_str(&format!("\n\n{}", desc));
                                }
                            }
                            if let Some(price) = price {
                                entry.push_str(&format!("\n\nPrice: {}", price));
                            }
                            structured_content.push(entry);
                        }
                    }
                    "Article" | "NewsArticle" | "BlogPosting" => {
                        let headline = self.json_ld_string(map.get("headline"));
                        let article_body = self.json_ld_string(map.get("articleBody"));
                        let author = self.json_ld_author(map.get("author"));

                        if let Some(body) = article_body {
                            if !body.trim().is_empty() {
                                let title = headline.unwrap_or_else(|| "Article".to_string());
                                let byline = author.unwrap_or_else(|| "".to_string());
                                if byline.is_empty() {
                                    structured_content.push(format!("# {}\n\n{}", title, body));
                                } else {
                                    structured_content
                                        .push(format!("# {}\n\nBy {}\n\n{}", title, byline, body));
                                }
                            }
                        }
                    }
                    "RealEstateListing" => {
                        let name = self
                            .json_ld_string(map.get("name"))
                            .unwrap_or_else(|| "Listing".to_string());
                        let address = self.json_ld_address(map.get("address"));
                        let price = self
                            .json_ld_string(map.get("price"))
                            .or_else(|| self.json_ld_price(map.get("offers")));

                        let mut entry = format!("# {}", name);
                        if let Some(address) = address {
                            entry.push_str(&format!("\n\nAddress: {}", address));
                        }
                        if let Some(price) = price {
                            entry.push_str(&format!("\nPrice: {}", price));
                        }
                        structured_content.push(entry);
                    }
                    _ => {
                        if let Some(name) = self.json_ld_string(map.get("name")) {
                            let description =
                                self.json_ld_string(map.get("description")).unwrap_or_default();
                            if description.is_empty() {
                                structured_content.push(name);
                            } else {
                                structured_content.push(format!("{}\n{}", name, description));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn json_ld_string(&self, value: Option<&serde_json::Value>) -> Option<String> {
        match value {
            Some(serde_json::Value::String(s)) => Some(s.trim().to_string()),
            Some(serde_json::Value::Number(n)) => Some(n.to_string()),
            Some(serde_json::Value::Array(items)) => {
                for item in items {
                    if let Some(val) = self.json_ld_string(Some(item)) {
                        if !val.is_empty() {
                            return Some(val);
                        }
                    }
                }
                None
            }
            Some(serde_json::Value::Object(map)) => {
                if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                    return Some(name.trim().to_string());
                }
                None
            }
            _ => None,
        }
    }

    fn json_ld_author(&self, value: Option<&serde_json::Value>) -> Option<String> {
        match value {
            Some(serde_json::Value::String(s)) => Some(s.trim().to_string()),
            Some(serde_json::Value::Array(items)) => {
                let mut names = Vec::new();
                for item in items {
                    if let Some(name) = self.json_ld_author(Some(item)) {
                        if !name.is_empty() {
                            names.push(name);
                        }
                    }
                }
                if names.is_empty() {
                    None
                } else {
                    Some(names.join(", "))
                }
            }
            Some(serde_json::Value::Object(map)) => {
                if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                    return Some(name.trim().to_string());
                }
                None
            }
            _ => None,
        }
    }

    fn json_ld_price(&self, value: Option<&serde_json::Value>) -> Option<String> {
        match value {
            Some(serde_json::Value::Object(map)) => {
                if let Some(price) = map.get("price") {
                    return self.json_ld_string(Some(price));
                }
                if let Some(price) = map.get("lowPrice") {
                    return self.json_ld_string(Some(price));
                }
                if let Some(price) = map.get("highPrice") {
                    return self.json_ld_string(Some(price));
                }
                if let Some(price) = map.get("offers") {
                    return self.json_ld_price(Some(price));
                }
                None
            }
            Some(serde_json::Value::Array(items)) => {
                for item in items {
                    if let Some(val) = self.json_ld_price(Some(item)) {
                        if !val.is_empty() {
                            return Some(val);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn json_ld_address(&self, value: Option<&serde_json::Value>) -> Option<String> {
        match value {
            Some(serde_json::Value::String(s)) => Some(s.trim().to_string()),
            Some(serde_json::Value::Object(map)) => {
                let street = map
                    .get("streetAddress")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let locality = map
                    .get("addressLocality")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let region = map
                    .get("addressRegion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let postal = map
                    .get("postalCode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let mut parts = Vec::new();
                if !street.is_empty() {
                    parts.push(street);
                }
                if !locality.is_empty() {
                    parts.push(locality);
                }
                if !region.is_empty() {
                    parts.push(region);
                }
                if !postal.is_empty() {
                    parts.push(postal);
                }
                if parts.is_empty() {
                    None
                } else {
                    Some(parts.join(", "))
                }
            }
            _ => None,
        }
    }
}
