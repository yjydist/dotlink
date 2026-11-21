//! 备份模块的线程局部测试接缝.
//!
//! 真实的嵌套复制冲突, 元数据写入失败与候选名检测失败无法在可移植的测试里稳定
//! 构造; 线程局部状态避免并行测试互相影响. 生产代码只在 `#[cfg(test)]` 下调用
//! 这里的注入点.

// 测试接缝: 让子项复制返回模拟的 `AlreadyExists` 分类 (`StagingNameTaken`).
//
// 注入只发生在 `copy_child` (子项递归入口), 不覆盖暂存根的排他创建; 返回的
// `StagingNameTaken` 会经过 `copy_child` 的 `Fatal` 包装.
thread_local! {
    static FAIL_NESTED_COPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static NESTED_COPY_FAILURES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// 在当前线程启用子项冲突注入, 直到返回的守卫被丢弃.
pub(super) fn fail_nested_child_copy() -> NestedCopyFailureGuard {
    let previous_active = FAIL_NESTED_COPY.with(|slot| slot.replace(true));
    let previous_count = NESTED_COPY_FAILURES.with(|slot| slot.replace(0));
    NestedCopyFailureGuard {
        previous_active,
        previous_count,
    }
}

pub(super) struct NestedCopyFailureGuard {
    previous_active: bool,
    previous_count: u32,
}

impl NestedCopyFailureGuard {
    /// 已注入次数; 若实现仍把嵌套冲突当作暂存名冲突重试, 该值会随重试增长.
    pub(super) fn injections(&self) -> u32 {
        NESTED_COPY_FAILURES.with(std::cell::Cell::get)
    }
}

impl Drop for NestedCopyFailureGuard {
    fn drop(&mut self) {
        FAIL_NESTED_COPY.with(|slot| slot.set(self.previous_active));
        NESTED_COPY_FAILURES.with(|slot| slot.set(self.previous_count));
    }
}

/// 测试注入点: 命中当前注入时记一次数, 让子项复制返回 `StagingNameTaken`, 由
/// `copy_child` 包装成带路径的 `Fatal`.
pub(super) fn take_nested_copy_failure() -> bool {
    if !FAIL_NESTED_COPY.with(std::cell::Cell::get) {
        return false;
    }
    NESTED_COPY_FAILURES.with(|slot| slot.set(slot.get() + 1));
    true
}

// 测试接缝: 让元数据应用阶段返回模拟的元数据写入失败.
//
// 注入命中后由调用点包装成 `CopyFailure::Fatal`, 因此不会触发暂存名换名重试.
thread_local! {
    static FAIL_METADATA: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static METADATA_FAILURES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// 在当前线程启用元数据失败注入, 直到返回的守卫被丢弃.
pub(super) fn fail_metadata() -> MetadataFailureGuard {
    let previous_active = FAIL_METADATA.with(|slot| slot.replace(true));
    let previous_count = METADATA_FAILURES.with(|slot| slot.replace(0));
    MetadataFailureGuard {
        previous_active,
        previous_count,
    }
}

pub(super) struct MetadataFailureGuard {
    previous_active: bool,
    previous_count: u32,
}

impl MetadataFailureGuard {
    /// 已注入次数; 元数据失败应立即结束复制, 因此用例断言恰为 1.
    pub(super) fn injections(&self) -> u32 {
        METADATA_FAILURES.with(std::cell::Cell::get)
    }
}

impl Drop for MetadataFailureGuard {
    fn drop(&mut self) {
        FAIL_METADATA.with(|slot| slot.set(self.previous_active));
        METADATA_FAILURES.with(|slot| slot.set(self.previous_count));
    }
}

/// 测试注入点: 命中当前注入时记一次数, 让 `apply_metadata` 返回错误.
pub(super) fn take_metadata_failure() -> bool {
    if !FAIL_METADATA.with(std::cell::Cell::get) {
        return false;
    }
    METADATA_FAILURES.with(|slot| slot.set(slot.get() + 1));
    true
}

// 测试接缝: 让下一次 `backup_path_for` 返回模拟失败.
//
// 真实的候选名占用检测错误 (EACCES / ENAMETOOLONG) 在清单校验 (V-12 / V-13) 之后
// 已无法通过 CLI 构造; 线程局部状态让 preview 的 `Err -> fail_row` 分支可被单独
// 锁定, 又避免并行测试互相影响.
thread_local! {
    static FAIL_BACKUP_PATH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 在当前线程启用一次预测备份名失败, 直到返回的守卫被丢弃.
pub(crate) fn fail_backup_path_once() -> BackupPathFailureGuard {
    let previous = FAIL_BACKUP_PATH.with(|slot| slot.replace(true));
    BackupPathFailureGuard { previous }
}

pub(crate) struct BackupPathFailureGuard {
    previous: bool,
}

impl Drop for BackupPathFailureGuard {
    fn drop(&mut self) {
        FAIL_BACKUP_PATH.with(|slot| slot.set(self.previous));
    }
}

/// 测试注入点: 命中一次后自动关闭, 使同一次 preview 的其余条目仍走真实路径.
pub(super) fn take_backup_path_failure() -> bool {
    FAIL_BACKUP_PATH.with(|slot| slot.replace(false))
}
