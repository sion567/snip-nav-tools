# sqlite3

> 轻量 SQL 数据库命令行：本地建库、跑查询、导入导出。
> 更多信息：<https://sqlite.org/cli.html>。

- 直接执行一条查询：

`sqlite3 {{库文件.db}} "SELECT * FROM users LIMIT 10"`

- 建表并插入数据：

`sqlite3 {{库文件.db}} "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT); INSERT INTO t(name) VALUES('demo');"`

- 表头 + 列对齐显示：

`sqlite3 -header -column {{库文件.db}} "SELECT * FROM users"`

- 导出整库为 SQL 文本：

`sqlite3 {{库文件.db}} .dump > backup.sql`

- 从 SQL 文本恢复数据库：

`sqlite3 {{新库.db}} < backup.sql`
