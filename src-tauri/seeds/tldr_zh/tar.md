# tar

> 打包/解包 tar 归档，常配合 gzip。
> 更多信息：<https://www.gnu.org/software/tar/>。

- 打包目录为 tar.gz：

`tar -czvf bundle.tar.gz {{目录}}`

- 解压 tar.gz 到当前目录：

`tar -xzvf bundle.tar.gz`

- 解压到指定目录：

`tar -xzvf bundle.tar.gz -C {{目标目录}}`

- 只看包内容不解压：

`tar -tzvf bundle.tar.gz`
