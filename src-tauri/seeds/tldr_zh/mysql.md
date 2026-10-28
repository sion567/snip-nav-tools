# mysql

> MySQL 客户端命令行：连库、跑查询、导入导出。
> 更多信息：<https://dev.mysql.com/doc/refman/8.0/en/mysql.html>。

- 连接数据库（交互模式，回车后输密码）：

`mysql -h {{主机}} -u {{用户}} -p {{库名}}`

- 直接执行一条查询：

`mysql -h {{主机}} -u {{用户}} -p -e "SELECT * FROM users LIMIT 10" {{库名}}`

- 执行 SQL 脚本文件（建库/初始化）：

`mysql -h {{主机}} -u {{用户}} -p {{库名}} < init.sql`

- 导出查询结果为 TSV：

`mysql -h {{主机}} -u {{用户}} -p -e "SELECT * FROM users" {{库名}} > users.tsv`

- 查看当前连接与正在跑的语句（排查慢查询/锁）：

`mysql -h {{主机}} -u {{用户}} -p -e "SHOW FULL PROCESSLIST"`
