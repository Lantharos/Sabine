use sabine_runtime::RuntimeInfo;

use super::manifest::development_env;
use super::origins::{allow_dev_origins, allow_url_origin};
use super::{SabineWindow, SabineWindowConfig};
use crate::desktop::open_urls::Openable;
use crate::desktop::{DesktopClaim, TRAY_UPDATE_COMMAND};
use crate::error::{SabineError, SabineResult};
use crate::host::SabineProcess;
use crate::launch::metrics::LaunchMetrics;
use crate::launch::{bootstrap, dev_server, local_entry, metrics_label};
use crate::osr;

impl SabineWindow {
    pub fn launch(mut self) -> SabineResult<SabineProcess> {
        crate::bridge::export_requested_bindings(&self.bridge_bindings);
        self.apply_launch_environment();
        let metrics = LaunchMetrics::new(metrics_label(&self.config));
        metrics.mark("launch.start");
        self.config.validate()?;
        metrics.mark("config.ready");
        let claim = DesktopClaim::acquire(&self.config)?;
        metrics.mark("single_instance.ready");
        let runtime = bootstrap::prepare(&self.config)?;
        metrics.mark("bootstrap.ready");
        self.launch_with_runtime(runtime, claim, metrics)
    }

    /// Resolve entry URL and config for [`SabineProcess::open_window`].
    /// Does not start desktop services or a second process island.
    pub(crate) fn into_open_window_parts(mut self) -> SabineResult<(SabineWindowConfig, String)> {
        self.apply_launch_environment();
        self.config.validate()?;
        self.allow_configured_url_origins();
        let url = self.entry_url()?;
        Ok((self.config, url))
    }

    fn launch_with_runtime(
        mut self,
        runtime: RuntimeInfo,
        claim: DesktopClaim,
        metrics: LaunchMetrics,
    ) -> SabineResult<SabineProcess> {
        let desktop_services = claim.start(&self.config)?;
        metrics.mark("desktop_services.ready");
        if let Some(tray) = desktop_services.tray() {
            self = self.bridge_handler(TRAY_UPDATE_COMMAND, move |command| {
                tray.update_from_page(command.params)
                    .map(|()| sabine_bridge::BridgeResponse::json(serde_json::Value::Null))
                    .map_err(sabine_bridge::BridgeError::new)
            });
        }
        let openable = Openable::new(
            &self.config.desktop_services.deep_links,
            &self.config.document_extensions,
        );
        let open_urls = self.config.open_urls.clone();
        open_urls.receive_arguments(
            &std::env::args().skip(1).collect::<Vec<_>>(),
            std::env::current_dir().ok().as_deref(),
            &openable,
        );
        self = self.bridge_handler("app.takeOpenUrls", move |_| {
            Ok(sabine_bridge::BridgeResponse::json(serde_json::json!(
                open_urls.take()
            )))
        });
        self.allow_configured_url_origins();
        let mut url = self.entry_url()?;
        if self.config.dev_url.is_some() {
            url = dev_server::wait(&url)?;
            allow_dev_origins(&mut self.config.security, &url);
            metrics.mark("dev_server.ready");
        }
        let mut process = osr::launch_process(
            runtime.location.path(),
            &self.config,
            &self.bridge_handlers,
            &url,
            metrics.clone(),
        )?;
        process.desktop_services = Some(desktop_services);
        if let Some(listener) = &self.config.launched {
            listener.deliver(process.handle());
        }
        process.start_desktop_event_forwarder(openable);
        metrics.mark("launch.ready");
        Ok(process)
    }

    fn apply_launch_environment(&mut self) {
        if self.config.dev_mode() {
            self.config.dev_url = development_env("SABINE_DEV_URL");
            let environment = crate::AppEnvironment::Development;
            if let Some(id) = &mut self.config.app_id {
                *id = environment.app_id(id);
            }
            if let Some(id) = &mut self.config.desktop_services.single_instance_id {
                *id = environment.app_id(id);
            }
            self.config.app_update = None;
            self.config.desktop_services.deep_links.clear();
            self.config.desktop_services.native_messaging_hosts.clear();
            self.config.desktop_services.autostart.clear();
        }
        if self.config.remember_size {
            super::remembered_size::restore(&mut self.config);
        }
    }

    pub(crate) fn entry_url(&self) -> SabineResult<String> {
        let page = if let Some(url) = self.config.dev_url.as_ref().or(self.config.url.as_ref()) {
            url.clone()
        } else if let Some(entry) = &self.config.entry {
            local_entry(entry)?.url
        } else {
            return Err(SabineError::CreationFailed {
                message: "CEF window has no entry or URL".to_string(),
            });
        };
        Ok(page + &self.config.content_suffix)
    }

    fn allow_configured_url_origins(&mut self) {
        if let Some(url) = self.config.url.clone() {
            allow_url_origin(&mut self.config.security, &url);
        }
        if let Some(url) = self.config.dev_url.clone() {
            allow_dev_origins(&mut self.config.security, &url);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::window::SabineWindow;

    fn allows(window: &SabineWindow, origin: &str) -> bool {
        window
            .config
            .security
            .allowed_origins
            .iter()
            .any(|allowed| allowed == origin)
    }

    #[test]
    fn url_sets_production_url_and_bridge_origin() {
        let window = SabineWindow::new().url("https://raday.lantharos.com/dashboard");

        assert_eq!(
            window.entry_url().unwrap(),
            "https://raday.lantharos.com/dashboard"
        );
        assert!(window.config.security.remote_content);
        assert!(allows(&window, "https://raday.lantharos.com"));
    }

    #[test]
    fn dev_url_takes_precedence_over_production_url() {
        let mut window = SabineWindow::new()
            .url("https://raday.lantharos.com")
            .content_suffix("#inspector");
        window.config.dev_url = Some("http://localhost:5173".into());
        window.allow_configured_url_origins();

        assert_eq!(
            window.entry_url().unwrap(),
            "http://localhost:5173#inspector"
        );
        assert!(allows(&window, "https://raday.lantharos.com"));
        assert!(allows(&window, "http://localhost:5173"));
    }
}
