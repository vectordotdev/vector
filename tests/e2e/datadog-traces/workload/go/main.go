// Inventory service: a child span under another service name and a server error for one order.
package main

import (
	"context"
	"net/http"
	"os/signal"
	"strings"
	"syscall"

	httptrace "github.com/DataDog/dd-trace-go/contrib/net/http/v2"
	"github.com/DataDog/dd-trace-go/v2/ddtrace/tracer"
)

func main() {
	tracer.Start()
	defer tracer.Stop()
	mux := httptrace.NewServeMux()
	mux.HandleFunc("/stock/", func(w http.ResponseWriter, r *http.Request) {
		span, _ := tracer.StartSpanFromContext(r.Context(), "stock.query",
			tracer.ServiceName("inventory-db"),
			tracer.SpanType("sql"),
			tracer.ResourceName("SELECT stock"))
		span.SetTag("db.rows", 7)
		span.Finish()
		if strings.HasSuffix(r.URL.Path, "/4") {
			http.Error(w, "out of stock", http.StatusInternalServerError)
			return
		}
		w.Write([]byte("ok"))
	})
	server := &http.Server{Addr: ":8080", Handler: mux}
	// Stop serving on `docker stop` so the deferred tracer.Stop flushes buffered spans.
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM)
	defer stop()
	go func() {
		<-ctx.Done()
		server.Shutdown(context.Background())
	}()
	if err := server.ListenAndServe(); err != http.ErrServerClosed {
		panic(err)
	}
}
