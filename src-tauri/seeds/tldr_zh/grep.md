# grep

> 在文件内容中查找匹配的文本行，支持正则表达式。
> 更多信息：<https://www.gnu.org/software/grep/>。

- 在文件中查找包含关键字的行：

`grep "ERROR" app.log`

- 忽略大小写并显示行号：

`grep -in "error" app.log`

- 递归查找目录下所有文件：

`grep -r "TODO" src/`

- 显示匹配行的上下文各 3 行：

`grep -C 3 "ERROR" app.log`

- 反向匹配（显示不包含关键字的行）：

`grep -v "DEBUG" app.log`

- 用扩展正则匹配多个关键字之一：

`grep -E "ERROR|WARN" app.log`
