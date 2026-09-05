# 运费计算可用性 demo

金额满 100 元免运费，否则 10 元；负数金额拒绝。只依赖 Python 标准库。

从仓库根目录执行：

```bash
python3 -m unittest discover -s fixtures/usability -v
cargo test -p sdd-cli --test usability -- --test-threads=1
```

第一条运行真实业务测试；第二条在独立临时 Git 项目中通过 CLI 从初始化走到归档，并覆盖恢复、修订、多任务选择和协议错误。Windows 使用 `python`。

`spec.json` 和 `plan.json` 是宿主回传示例，不是供用户手工维护的项目内部状态。完整说明见 [可用性试用与回归](../../docs/usability.md)。
