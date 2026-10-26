# curl

> 向服务器发送 HTTP 请求，常用于调试接口。
> 更多信息：<https://curl.se>。

- 发 GET 请求并显示响应：

`curl https://example.com/api`

- 带 Header 的 GET 请求：

`curl -H "Authorization: Bearer {{token}}" https://example.com/api/me`

- 发 POST 请求（JSON body）：

`curl -X POST -H "Content-Type: application/json" -d '{"name":"test"}' https://example.com/api`

- 只看响应头：

`curl -I https://example.com`

- 显示详细连接过程（排查握手/重定向）：

`curl -v https://example.com`
