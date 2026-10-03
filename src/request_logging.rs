use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::Error;
use tracing::Span;
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};

/// Root span for every HTTP request, minus the query string.
///
/// `DefaultRootSpanBuilder` records `http.target` as path *and query*, and the
/// JSON log layer attaches that span to every event emitted while handling the
/// request. Query strings carry credentials here (`/ws?token=<JWT>` from older
/// clients, `/auth/reset-password?token=...`), so `http.target` is the path
/// only. Everything else follows the default builder.
pub struct PathOnlyRootSpanBuilder;

impl RootSpanBuilder for PathOnlyRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let user_agent = request
            .headers()
            .get("User-Agent")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let route = request.match_pattern().unwrap_or_else(|| "default".into());
        let client_ip = request
            .connection_info()
            .realip_remote_addr()
            .unwrap_or("")
            .to_string();
        tracing::info_span!(
            "HTTP request",
            http.method = %request.method(),
            http.route = %route,
            http.host = %request.connection_info().host(),
            http.client_ip = %client_ip,
            http.user_agent = %user_agent,
            http.target = %request.path(),
            http.status_code = tracing::field::Empty,
            otel.status_code = tracing::field::Empty,
            exception.message = tracing::field::Empty,
            exception.details = tracing::field::Empty,
        )
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, Error>) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}

/// Shared helpers for tests that assert on what the server writes to its logs.
#[cfg(test)]
pub mod test_support {
    use std::io;
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::fmt::MakeWriter;

    /// In-memory log sink: the JSON fmt layer used in `main`, minus stdout.
    #[derive(Clone, Default)]
    pub struct LogBuffer(Arc<Mutex<Vec<u8>>>);

    impl LogBuffer {
        pub fn contents(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }

        /// Route every event on this thread into the buffer until the guard drops.
        pub fn capture(&self) -> tracing::subscriber::DefaultGuard {
            tracing::subscriber::set_default(
                tracing_subscriber::fmt()
                    .json()
                    .with_max_level(tracing::Level::TRACE)
                    .with_writer(self.clone())
                    .finish(),
            )
        }
    }

    pub struct LogWriter(Arc<Mutex<Vec<u8>>>);

    impl io::Write for LogWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for LogBuffer {
        type Writer = LogWriter;
        fn make_writer(&'a self) -> Self::Writer {
            LogWriter(self.0.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{test, web, App, HttpResponse};
    use tracing_actix_web::TracingLogger;

    use super::test_support::LogBuffer;
    use super::PathOnlyRootSpanBuilder;

    const SECRET: &str = "eyJhbGciOiJIUzI1NiJ9.SECRET-PAYLOAD.SECRET-SIG";

    async fn handler_that_logs() -> HttpResponse {
        tracing::warn!("event emitted inside the request span");
        HttpResponse::Ok().finish()
    }

    #[actix_web::test]
    async fn request_log_context_keeps_the_path_but_never_the_query_string() {
        let logs = LogBuffer::default();
        let _guard = logs.capture();
        let app = test::init_service(
            App::new()
                .wrap(TracingLogger::<PathOnlyRootSpanBuilder>::new())
                .route("/ws", web::get().to(handler_that_logs)),
        )
        .await;

        let uri = format!("/ws?token={SECRET}&ticket={SECRET}");
        let res = test::call_service(&app, test::TestRequest::get().uri(&uri).to_request()).await;
        assert!(res.status().is_success());

        let out = logs.contents();
        assert!(
            out.contains("event emitted inside the request span"),
            "{out}"
        );
        assert!(
            out.contains(r#""http.target":"/ws""#),
            "path missing: {out}"
        );
        assert!(
            !out.contains("SECRET"),
            "query string leaked into logs: {out}"
        );
        assert!(
            !out.contains("/ws?"),
            "query string leaked into logs: {out}"
        );
    }

    /// Guards the guard: the stock builder does leak, so a green test above
    /// means the custom builder is what keeps the secret out.
    #[actix_web::test]
    async fn default_tracing_logger_would_have_leaked_the_query_string() {
        let logs = LogBuffer::default();
        let _guard = logs.capture();
        let app = test::init_service(
            App::new()
                .wrap(TracingLogger::default())
                .route("/ws", web::get().to(handler_that_logs)),
        )
        .await;

        let uri = format!("/ws?token={SECRET}");
        test::call_service(&app, test::TestRequest::get().uri(&uri).to_request()).await;

        assert!(
            logs.contents().contains("SECRET"),
            "premise broken: stock TracingLogger no longer logs the query"
        );
    }
}
