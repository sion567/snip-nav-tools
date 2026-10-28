# gradle

> Gradle 构建工具；项目里推荐用自带包装器 `./gradlew` 执行同样的任务。
> 更多信息：<https://docs.gradle.org>。

- 清理并打包（跳过测试）：

`./gradlew clean build -x test`

- 只跑单元测试：

`./gradlew test`

- 列出所有可用任务：

`./gradlew tasks --all`

- Spring Boot 指定端口启动：

`./gradlew bootRun --args="--server.port=8081"`

- 查看依赖树（排查版本冲突）：

`./gradlew dependencies --configuration runtimeClasspath`
