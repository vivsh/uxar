document.addEventListener("htmx:configRequest", (event) => {
  const csrf = document.querySelector("meta[name='csrf-token']");
  if (csrf) {
    event.detail.headers["x-csrf-token"] = csrf.content;
  }
});
