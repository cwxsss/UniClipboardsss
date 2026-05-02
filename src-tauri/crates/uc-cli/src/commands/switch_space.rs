//! `uniclip switch-space` — 已 setup 的设备加入另一个 sponsor 空间，
//! 同时把本地剪贴板历史从旧 master_key 重加密到新空间的 master_key。
//!
//! 与 [`super::join`] 的区别：
//! * `join` 假设设备处于 fresh 状态，handshake 后直接落 setup_status。
//! * `switch-space` 要求设备已经完成首次 setup（`init` 或 `join` 过），
//!   走的是 4 阶段重加密迁移路径，详见
//!   [`uc_application::usecases::setup::switch_space`]。
//!
//! 命令是一个阻塞 RPC，由 4 阶段 use case 内部按顺序跑完——CLI 只展示
//! 一个 spinner 等待，不需要轮询进度（粗粒度进度查询走 `AppFacade::
//! query_migration_progress`，留给 GUI 用）。

use tokio::select;
use tokio::signal;

use uc_application::facade::space_setup::{SwitchSpaceError, SwitchSpaceInput};

use crate::commands::app_session::{build_app_session, refuse_if_daemon_running};
use crate::exit_codes;
use crate::ui;

const EXIT_SIGINT: i32 = 130;

pub struct SwitchSpaceArgs {
    pub code: Option<String>,
    pub new_passphrase: Option<String>,
}

pub async fn run(args: SwitchSpaceArgs, verbose: bool) -> i32 {
    ui::header("Switch to another space");

    if let Err(code) = refuse_if_daemon_running().await {
        return code;
    }

    let bundle = match build_app_session(verbose).await {
        Ok(b) => b,
        Err(code) => return code,
    };

    let code_str = match args.code {
        Some(c) if !c.trim().is_empty() => c.trim().to_string(),
        Some(_) => {
            ui::error("--code is empty");
            bundle.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
        None => match ui::password("Invitation code from new sponsor") {
            Ok(c) if !c.trim().is_empty() => c.trim().to_string(),
            Ok(_) => {
                ui::error("Invitation code cannot be empty");
                bundle.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
            Err(e) => {
                ui::error(&e);
                bundle.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
        },
    };

    let new_passphrase = match args.new_passphrase {
        Some(p) if !p.trim().is_empty() => p,
        Some(_) => {
            ui::error("--new-passphrase is empty");
            bundle.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
        None => match ui::password("New space passphrase") {
            Ok(p) if !p.trim().is_empty() => p,
            Ok(_) => {
                ui::error("Passphrase cannot be empty");
                bundle.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
            Err(e) => {
                ui::error(&e);
                bundle.shutdown().await;
                return exit_codes::EXIT_ERROR;
            }
        },
    };

    // 启动期解锁本机当前空间——switch-space phase 1 要用旧 master_key 解
    // 历史密文，而 build_app_session 不会自动 unlock。`try_resume_session`
    // 利用 keyring 缓存的 KEK 静默解锁；如果失败说明设备还没 setup（用户
    // 应该跑 `init` 或 `join`）或 keyring 不可用。
    let resumed = match bundle.app_facade().try_resume_session().await {
        Ok(b) => b,
        Err(err) => {
            ui::error(&format!("Failed to resume session: {err}"));
            ui::info(
                "hint",
                "Run `uniclip init` or `uniclip join` first if this device is not set up.",
            );
            bundle.shutdown().await;
            return exit_codes::EXIT_ERROR;
        }
    };
    if !resumed {
        ui::error(
            "This device is not set up yet. Use `uniclip init` to create a new space, \
             or `uniclip join` to join one for the first time.",
        );
        bundle.shutdown().await;
        return exit_codes::EXIT_ERROR;
    }

    let input = SwitchSpaceInput {
        code: code_str,
        new_passphrase,
    };

    let spinner = ui::spinner(
        "Migrating local clipboard history to the new space (4 phases — this may take a while)...",
    );

    // Clone the Arc so the in-flight future does not borrow `bundle`.
    let facade = std::sync::Arc::clone(bundle.app_facade());
    let switch = async move { facade.switch_space(input).await };
    tokio::pin!(switch);

    let exit = select! {
        result = &mut switch => match result {
            Ok(out) => {
                ui::spinner_finish_success(&spinner, "Switched space");
                ui::info("space_id", out.space_id.as_str());
                ui::info("self_device_id", out.self_device_id.as_str());
                ui::info("self_fingerprint", &out.self_identity_fingerprint.to_string());
                ui::info("sponsor_device_id", out.sponsor_device_id.as_str());
                ui::info("sponsor_fingerprint", &out.sponsor_identity_fingerprint.to_string());
                ui::info(
                    "migrated_records",
                    &out.migrated_records.to_string(),
                );
                exit_codes::EXIT_SUCCESS
            }
            Err(err) => {
                let hint = match &err {
                    SwitchSpaceError::NotSetup => {
                        "This device must be set up first. Run `uniclip init` or `uniclip join`."
                    }
                    SwitchSpaceError::PendingMigration(_) => {
                        "A previous switch-space migration is still in flight. Restart `uniclip` to auto-resume, or factory-reset to abandon."
                    }
                    SwitchSpaceError::NotUnlocked => {
                        "Space session is locked. Restart `uniclip` so the keyring can re-unlock it before retrying."
                    }
                    SwitchSpaceError::InvitationNotFound => {
                        "Double-check the code — sponsor may have let it expire or reissued."
                    }
                    SwitchSpaceError::InvitationExpired => {
                        "Ask the new sponsor to run `invite` again to issue a fresh code."
                    }
                    SwitchSpaceError::PassphraseMismatch => {
                        "Passphrase did not match the new sponsor's. Retry."
                    }
                    SwitchSpaceError::SponsorUnreachable => {
                        "New sponsor is online in rendezvous but could not be reached. Check NAT / relay."
                    }
                    SwitchSpaceError::ServiceUnavailable => {
                        "Rendezvous service is unreachable."
                    }
                    SwitchSpaceError::InvalidCiphertext => {
                        "Local clipboard data could not be decrypted under the current key. \
                         The space may already have been partially migrated by a previous run; \
                         restart `uniclip` to auto-resume."
                    }
                    _ => "",
                };
                ui::spinner_finish_error(&spinner, &format!("Switch-space failed: {err}"));
                if !hint.is_empty() {
                    ui::info("hint", hint);
                }
                exit_codes::EXIT_ERROR
            }
        },
        _ = signal::ctrl_c() => {
            ui::spinner_finish_error(&spinner, "Interrupted by user");
            ui::info(
                "note",
                "Migration may be partially complete. Restart `uniclip` to auto-resume.",
            );
            EXIT_SIGINT
        }
    };

    bundle.shutdown().await;
    exit
}
