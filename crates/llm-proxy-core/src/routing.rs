use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRoute {
    pub route_name: String,
    pub upstream_model: String,
    pub routing_match: RoutingMatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMatch {
    Explicit,
    Default,
}

pub fn resolve_route(config: &Config, requested_model: &str) -> ResolvedRoute {
    if let Some(model_route) = config.models.get(requested_model) {
        return ResolvedRoute {
            route_name: model_route.route.clone(),
            upstream_model: model_route
                .upstream_model
                .clone()
                .unwrap_or_else(|| requested_model.to_owned()),
            routing_match: RoutingMatch::Explicit,
        };
    }

    ResolvedRoute {
        route_name: config.default_route.clone(),
        upstream_model: requested_model.to_owned(),
        routing_match: RoutingMatch::Default,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{config::ModelRoute, Config};

    use super::*;

    #[test]
    fn unknown_model_uses_default_route_unchanged() {
        let config = Config::default();

        let resolved = resolve_route(&config, "unknown-model");

        assert_eq!(resolved.route_name, "local");
        assert_eq!(resolved.upstream_model, "unknown-model");
        assert_eq!(resolved.routing_match, RoutingMatch::Default);
    }

    #[test]
    fn explicit_model_can_rewrite_upstream_model() {
        let mut models = BTreeMap::new();
        models.insert(
            "fast-local".to_owned(),
            ModelRoute {
                route: "local".to_owned(),
                upstream_model: Some("llama-3.1-8b-instruct".to_owned()),
            },
        );
        let config = Config {
            models,
            ..Config::default()
        };

        let resolved = resolve_route(&config, "fast-local");

        assert_eq!(resolved.route_name, "local");
        assert_eq!(resolved.upstream_model, "llama-3.1-8b-instruct");
        assert_eq!(resolved.routing_match, RoutingMatch::Explicit);
    }
}
