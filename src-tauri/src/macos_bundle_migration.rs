use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const LEGACY_BUNDLE_NAME: &str = "CC Switch.app";
const CURRENT_BUNDLE_NAME: &str = "Agent Switch.app";

#[derive(Debug, Clone, PartialEq, Eq)]
struct BundleMigrationPlan {
    legacy_bundle: PathBuf,
    current_bundle: PathBuf,
}

fn migration_plan_for_executable(executable: &Path) -> Option<BundleMigrationPlan> {
    let legacy_bundle = executable.ancestors().find(|path| {
        path.file_name()
            .is_some_and(|name| name == LEGACY_BUNDLE_NAME)
    })?;
    let parent = legacy_bundle.parent()?;

    Some(BundleMigrationPlan {
        legacy_bundle: legacy_bundle.to_path_buf(),
        current_bundle: parent.join(CURRENT_BUNDLE_NAME),
    })
}

fn rename_legacy_bundle(plan: &BundleMigrationPlan) -> io::Result<()> {
    if plan.current_bundle.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "目标应用已存在，拒绝覆盖: {}",
                plan.current_bundle.display()
            ),
        ));
    }

    fs::rename(&plan.legacy_bundle, &plan.current_bundle).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "无法将旧应用包 {} 重命名为 {}: {error}",
                plan.legacy_bundle.display(),
                plan.current_bundle.display()
            ),
        )
    })
}

#[cfg(target_os = "macos")]
fn register_bundle(bundle: &Path) {
    use std::process::{Command, Stdio};

    const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";

    match Command::new(LSREGISTER)
        .arg("-f")
        .arg(bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!(
            "注册改名后的 macOS 应用包失败（exit={status}），将继续尝试启动: {}",
            bundle.display()
        ),
        Err(error) => eprintln!(
            "无法调用 LaunchServices 注册改名后的应用包，将继续尝试启动 {}: {error}",
            bundle.display()
        ),
    }
}

#[cfg(target_os = "macos")]
fn relaunch_bundle(bundle: &Path) -> io::Result<()> {
    use std::process::{Command, Stdio};

    let mut command = Command::new("/usr/bin/open");
    command
        .arg("-n")
        .arg(bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let forwarded_args: Vec<_> = std::env::args_os().skip(1).collect();
    if !forwarded_args.is_empty() {
        command.arg("--args").args(forwarded_args);
    }

    command.spawn().map(|_| ()).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "应用包已改名，但无法从新路径启动 {}: {error}",
                bundle.display()
            ),
        )
    })
}

/// 将从旧版本原地更新后遗留的 `CC Switch.app` 一次性改名为
/// `Agent Switch.app`，并从新路径重新启动。
///
/// 必须在 Tauri Builder 和 single-instance 插件初始化之前调用，避免新实例
/// 被当前进程持有的单实例锁拦截。返回 `true` 表示已完成改名并安排重启，调用方
/// 应立即退出当前进程；不在旧应用包中运行时返回 `false`。
#[cfg(target_os = "macos")]
pub(crate) fn migrate_legacy_bundle_and_relaunch() -> io::Result<bool> {
    let executable = std::env::current_exe().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("无法获取当前 macOS 可执行文件路径: {error}"),
        )
    })?;

    let Some(plan) = migration_plan_for_executable(&executable) else {
        return Ok(false);
    };

    rename_legacy_bundle(&plan)?;
    register_bundle(&plan.current_bundle);

    if let Err(relaunch_error) = relaunch_bundle(&plan.current_bundle) {
        return match fs::rename(&plan.current_bundle, &plan.legacy_bundle) {
            Ok(()) => Err(relaunch_error),
            Err(rollback_error) => Err(io::Error::other(format!(
                "{relaunch_error}; 回滚应用包名称也失败: {rollback_error}"
            ))),
        };
    }

    eprintln!(
        "已将 macOS 应用包从 {} 迁移到 {}，正在从新路径重新启动",
        plan.legacy_bundle.display(),
        plan.current_bundle.display()
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_migration_for_legacy_bundle() {
        let executable = Path::new("/Applications/CC Switch.app/Contents/MacOS/agent-switch");

        assert_eq!(
            migration_plan_for_executable(executable),
            Some(BundleMigrationPlan {
                legacy_bundle: PathBuf::from("/Applications/CC Switch.app"),
                current_bundle: PathBuf::from("/Applications/Agent Switch.app"),
            })
        );
    }

    #[test]
    fn ignores_current_bundle_and_development_binary() {
        assert_eq!(
            migration_plan_for_executable(Path::new(
                "/Applications/Agent Switch.app/Contents/MacOS/agent-switch"
            )),
            None
        );
        assert_eq!(
            migration_plan_for_executable(Path::new("/workspace/target/debug/agent-switch")),
            None
        );
    }

    #[test]
    fn renames_bundle_without_touching_contents() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_bundle = temp.path().join(LEGACY_BUNDLE_NAME);
        let current_bundle = temp.path().join(CURRENT_BUNDLE_NAME);
        fs::create_dir_all(legacy_bundle.join("Contents/MacOS")).unwrap();
        fs::write(
            legacy_bundle.join("Contents/MacOS/agent-switch"),
            b"payload",
        )
        .unwrap();

        let plan = BundleMigrationPlan {
            legacy_bundle: legacy_bundle.clone(),
            current_bundle: current_bundle.clone(),
        };
        rename_legacy_bundle(&plan).unwrap();

        assert!(!legacy_bundle.exists());
        assert_eq!(
            fs::read(current_bundle.join("Contents/MacOS/agent-switch")).unwrap(),
            b"payload"
        );
    }

    #[test]
    fn refuses_to_overwrite_existing_current_bundle() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_bundle = temp.path().join(LEGACY_BUNDLE_NAME);
        let current_bundle = temp.path().join(CURRENT_BUNDLE_NAME);
        fs::create_dir_all(&legacy_bundle).unwrap();
        fs::create_dir_all(&current_bundle).unwrap();

        let error = rename_legacy_bundle(&BundleMigrationPlan {
            legacy_bundle: legacy_bundle.clone(),
            current_bundle: current_bundle.clone(),
        })
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(legacy_bundle.exists());
        assert!(current_bundle.exists());
    }
}
