defmodule Wasmex.Components.HttpHandlerTest do
  use ExUnit.Case, async: false

  alias Wasmex.Components
  alias Wasmex.Wasi.WasiP2Options

  @moduletag :wasi_p2
  @component_path "test/component_fixtures/http_handler_test/http-handler.wasm"

  describe "call_http_handler/6" do
    test "handles GET request to root path" do
      component_bytes = File.read!(@component_path)

      {:ok, pid} =
        Components.start_link(%{
          bytes: component_bytes,
          wasi: %WasiP2Options{allow_http: true}
        })

      {:ok, {status, headers, body}} =
        Components.call_http_handler(
          pid,
          "GET",
          "/",
          [],
          ""
        )

      assert status == 200
      assert is_list(headers)
      assert is_binary(body)

      # The default Hono template responds with JSON
      assert String.contains?(body, "Hello from TypeScript + Hono!")
    end

    test "handles GET request with path parameters" do
      component_bytes = File.read!(@component_path)

      {:ok, pid} =
        Components.start_link(%{
          bytes: component_bytes,
          wasi: %WasiP2Options{allow_http: true}
        })

      {:ok, {status, headers, body}} =
        Components.call_http_handler(
          pid,
          "GET",
          "/hello/Wasmex",
          [],
          ""
        )

      assert status == 200
      assert is_list(headers)
      assert String.contains?(body, "Hello, Wasmex")
    end

    test "handles 404 for unknown paths" do
      component_bytes = File.read!(@component_path)

      {:ok, pid} =
        Components.start_link(%{
          bytes: component_bytes,
          wasi: %WasiP2Options{allow_http: true}
        })

      {:ok, {status, headers, body}} =
        Components.call_http_handler(
          pid,
          "GET",
          "/nonexistent",
          [],
          ""
        )

      assert status == 404
      assert is_list(headers)
      assert is_binary(body)
    end

    test "handles custom headers" do
      component_bytes = File.read!(@component_path)

      {:ok, pid} =
        Components.start_link(%{
          bytes: component_bytes,
          wasi: %WasiP2Options{allow_http: true}
        })

      {:ok, {status, headers, _body}} =
        Components.call_http_handler(
          pid,
          "GET",
          "/",
          [{"x-custom-header", "test-value"}],
          ""
        )

      assert status == 200
      assert is_list(headers)

      # Check that response has content-type header
      assert Enum.any?(headers, fn {name, _value} ->
               String.downcase(name) == "content-type"
             end)
    end

    test "can make multiple requests to the same component instance" do
      component_bytes = File.read!(@component_path)

      {:ok, pid} =
        Components.start_link(%{
          bytes: component_bytes,
          wasi: %WasiP2Options{allow_http: true}
        })

      # First request
      {:ok, {status1, _, body1}} =
        Components.call_http_handler(pid, "GET", "/", [], "")

      assert status1 == 200
      assert String.contains?(body1, "Hello from TypeScript + Hono!")

      # Second request
      {:ok, {status2, _, body2}} =
        Components.call_http_handler(pid, "GET", "/hello/World", [], "")

      assert status2 == 200
      assert String.contains?(body2, "Hello, World")

      # Third request
      {:ok, {status3, _, _body3}} =
        Components.call_http_handler(pid, "GET", "/nonexistent", [], "")

      assert status3 == 404
    end
  end
end
