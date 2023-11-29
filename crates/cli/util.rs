pub fn install_tracing(log_level: Option<tracing::Level>) {
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::prelude::*;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_filter(LevelFilter::from(
                    log_level.unwrap_or(tracing::Level::DEBUG),
                )),
        )
        .init();
}
