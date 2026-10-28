# mvn

> Apache Maven 构建工具：编译、测试、打包 Java 项目。
> 更多信息：<https://maven.apache.org>。

- 清理并打包（跳过测试，日常最快出包）：

`mvn clean package -DskipTests`

- 只跑单元测试：

`mvn test`

- 安装到本地仓库（供其他项目依赖）：

`mvn clean install -DskipTests`

- 查看完整依赖树（排查版本冲突）：

`mvn dependency:tree`

- 离线编译（依赖已拉齐时给 CI 提速）：

`mvn -o clean compile`
