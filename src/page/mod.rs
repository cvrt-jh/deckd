use crate::config::schema::{AppConfig, ButtonConfig, PageConfig};
use tracing::{debug, info};

/// Manages the page stack and provides button lookups.
pub struct PageManager {
    /// Stack of page IDs. Last element is the current page.
    stack: Vec<String>,
    home_page: String,
}

impl PageManager {
    pub fn new(home_page: &str) -> Self {
        Self {
            stack: vec![home_page.to_string()],
            home_page: home_page.to_string(),
        }
    }

    /// Get the current page ID.
    pub fn current_page(&self) -> &str {
        self.stack
            .last()
            .map(|s| s.as_str())
            .unwrap_or(&self.home_page)
    }

    /// Navigate to a page by ID, pushing onto the stack.
    pub fn navigate_to(&mut self, page_id: &str) {
        info!("navigate: {} → {page_id}", self.current_page());
        self.stack.push(page_id.to_string());
    }

    /// Go back one page. Returns true if the page changed.
    pub fn go_back(&mut self) -> bool {
        if self.stack.len() > 1 {
            let from = self.stack.pop().unwrap();
            info!("navigate back: {from} → {}", self.current_page());
            true
        } else {
            debug!("already at home page, cannot go back");
            false
        }
    }

    /// Reset to home page.
    pub fn go_home(&mut self) {
        info!("navigate home");
        self.stack.clear();
        self.stack.push(self.home_page.clone());
    }

    /// Look up the current page config.
    pub fn current_page_config<'a>(&self, config: &'a AppConfig) -> Option<&'a PageConfig> {
        config.pages.get(self.current_page())
    }

    /// Look up a button config by key index on the current page.
    pub fn button_for_key<'a>(&self, config: &'a AppConfig, key: u8) -> Option<&'a ButtonConfig> {
        self.current_page_config(config)?
            .buttons
            .iter()
            .find(|b| b.key == key)
    }

    /// Update home page (e.g., after config reload).
    pub fn set_home_page(&mut self, home: &str) {
        self.home_page = home.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_stack() {
        let mut pm = PageManager::new("home");
        assert_eq!(pm.current_page(), "home");

        pm.navigate_to("lights");
        assert_eq!(pm.current_page(), "lights");

        pm.navigate_to("scenes");
        assert_eq!(pm.current_page(), "scenes");

        assert!(pm.go_back());
        assert_eq!(pm.current_page(), "lights");

        pm.go_home();
        assert_eq!(pm.current_page(), "home");

        // Can't go back from home.
        assert!(!pm.go_back());
    }
}
