//! MCP (Model Context Protocol) utilities
//! 
//! The MCP lets you build servers that expose data and functionality to LLM applications in a secure, standardized way.

use crate::App;
use crate::http::{
    endpoints::{
        args::FromRequest,
        handlers::{Func, GenericHandler},
    },
    IntoResponse,
    Method
};

impl App {
    pub fn map_tool<F, R, Args>(&mut self, name: &str, handler: F) -> &mut Self
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + Sync + 'static
    {
        self.map_get(name, handler);
        self
    }

    pub fn map_resource<F, R, Args>(&mut self, pattern: &str, handler: F) -> &mut Self
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + Sync + 'static
    {
        self.map_post(pattern, handler);
        self
    }

    pub fn map_prompt<F, R, Args>(&mut self, name: &str, handler: F) -> &mut Self
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + Sync + 'static
    {
        self.map_get(name, handler);
        self
    }
}

macro_rules! method_name {
    () => {{
        let name = std::any::type_name::<fn()>();
        name.split("::").last().unwrap()
    }};
}