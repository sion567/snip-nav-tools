# jq

> 命令行 JSON 处理器：过滤、转换、格式化 JSON。
> 更多信息：<https://jqlang.github.io/jq/>。

- 格式化 JSON（美化输出）：

`cat data.json | jq .`

- 取数组里每个对象的某个字段：

`cat pods.json | jq '.items[].metadata.name'`

- 过滤数组中满足条件的元素：

`cat logs.json | jq '.[] | select(.level == "ERROR")'`

- 取 JSON 日志的多个字段：

`cat app.log | jq '{time, level, msg}'`
