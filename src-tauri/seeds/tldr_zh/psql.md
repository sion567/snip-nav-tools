# psql

> PostgreSQL 交互式终端：连库、跑查询、导入导出。
> 更多信息：<https://www.postgresql.org/docs/current/app-psql.html>。

- 连接数据库（交互模式）：

`psql -h {{主机}} -U {{用户}} -d {{库名}}`

- 直接执行一条查询：

`psql -d {{库名}} -c "SELECT * FROM users LIMIT 10"`

- 执行 SQL 脚本文件：

`psql -d {{库名}} -f init.sql`

- 列出所有表：

`psql -d {{库名}} -c "\dt"`

- 导出查询结果为 CSV：

`psql -d {{库名}} -c "\copy (SELECT * FROM users) TO 'users.csv' CSV HEADER"`
