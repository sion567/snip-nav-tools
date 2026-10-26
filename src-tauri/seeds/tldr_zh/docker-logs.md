# docker logs

> 查看容器的标准输出日志。
> 更多信息：<https://docs.docker.com/reference/cli/docker/container/logs/>。

- 查看容器全部日志：

`docker logs {{容器}}`

- 实时跟随最新输出：

`docker logs -f --tail 100 {{容器}}`

- 只看最近 1 小时并带时间戳：

`docker logs --since 1h -t {{容器}}`

- 只看指定时间之后的日志：

`docker logs --since 2026-10-25T00:00:00 {{容器}}`
