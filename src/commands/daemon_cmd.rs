use crate::cli::{DaemonAction, DaemonArgs, ServeArgs};
use crate::daemon;
use crate::error::CliError;
use crate::output::{Format, Outcome, pretty};
use crate::transport::Ctx;

pub async fn serve(ctx: &Ctx, args: ServeArgs) -> Result<Outcome, CliError> {
    let summary = daemon::serve(ctx, args.idle).await?;
    Ok(Outcome::render(ctx.out(Format::Json), summary, pretty))
}

pub async fn daemon(ctx: &Ctx, args: DaemonArgs) -> Result<Outcome, CliError> {
    let value = match args.action {
        DaemonAction::Status => daemon::status(ctx).await?,
        DaemonAction::Stop => daemon::stop(ctx).await?,
    };
    Ok(Outcome::render(ctx.out(Format::Json), value, pretty))
}
