//! One function per subcommand. Each returns an `Outcome` (what to print and
//! whether an error still has to be reported) or a hard `CliError`.

mod build;
mod catalog;
mod daemon_cmd;
mod decompile;
mod doctor;
mod format;
mod meta;
mod skill;

use crate::cli::Command;
use crate::error::CliError;
use crate::output::Outcome;
use crate::transport::Ctx;

pub async fn dispatch(ctx: &Ctx, command: Command) -> Result<Outcome, CliError> {
    match command {
        Command::Build(args) => build::build(ctx, args).await,
        Command::BuildParams(args) => build::build_params(ctx, args).await,
        Command::Snapshot(args) => build::snapshot(ctx, args).await,
        Command::Format(args) => format::format(ctx, args).await,
        Command::Refs(args) => format::refs(ctx, args).await,
        Command::Decompile(args) => decompile::decompile(ctx, args, false).await,
        Command::DecompileParams(args) => decompile::decompile(ctx, args, true).await,
        Command::ResourceTypes(args) => catalog::resource_types(ctx, args).await,
        Command::Schema(args) => catalog::schema(ctx, args).await,
        Command::Extensions(args) => catalog::extensions(ctx, args).await,
        Command::ExtTypes(args) => catalog::ext_types(ctx, args).await,
        Command::ExtSchema(args) => catalog::ext_schema(ctx, args).await,
        Command::Avm(args) => catalog::avm(ctx, args).await,
        Command::BestPractices => catalog::best_practices(ctx).await,
        Command::Tools(args) => meta::tools(ctx, args).await,
        Command::Call(args) => meta::call(ctx, args).await,
        Command::Batch => meta::batch(ctx).await,
        Command::Serve(args) => daemon_cmd::serve(ctx, args).await,
        Command::Daemon(args) => daemon_cmd::daemon(ctx, args).await,
        Command::Skill(args) => skill::run(ctx, args.action),
        Command::Doctor => doctor::doctor(ctx).await,
        Command::Version(args) => doctor::version(ctx, args).await,
    }
}

/// Shared shape for "invoke or return raw" used by most commands.
macro_rules! invoke_or_raw {
    ($ctx:expr, $tool:expr, $args:expr) => {
        match $ctx.invoke($tool, $args).await? {
            $crate::transport::Invoked::Raw(v, err) => {
                let outcome = $crate::output::Outcome::json(v);
                return Ok(match err {
                    Some(e) => outcome.with_error(e),
                    None => outcome,
                });
            }
            $crate::transport::Invoked::Value(v) => v,
        }
    };
}
pub(crate) use invoke_or_raw;

pub(crate) fn write_file(path: &std::path::Path, content: &str) -> Result<(), CliError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::io(&format!("cannot create {}", parent.display()), e))?;
    }
    std::fs::write(path, content)
        .map_err(|e| CliError::io(&format!("cannot write {}", path.display()), e))
}
