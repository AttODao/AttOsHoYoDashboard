package main

import (
	"github.com/danielgtaylor/huma/v2"
	"github.com/danielgtaylor/huma/v2/adapters/humaecho"
	"github.com/labstack/echo/v5"
)

func main() {
	e := echo.New()
	config := huma.DefaultConfig("User API", "1.0.0")
	api := humaecho.New(e, config)

	if err := e.Start(":8080"); err != nil {
		e.Logger.Error(
			"server stopped",
			"error", err,
		)
	}
}
