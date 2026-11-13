const $ = (sel) => document.querySelector(sel);

const state = {
  token: localStorage.getItem("token"),
  email: localStorage.getItem("email"),
  page: 1,
  perPage: 50,
  total: 0,
  mode: "login",
  expenses: [],
  currentExpense: null,
};

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

async function api(path, opts = {}) {
  const headers = { ...opts.headers };
  if (!(opts.body instanceof FormData) && opts.body) headers["Content-Type"] = "application/json";
  if (state.token) headers["Authorization"] = `Bearer ${state.token}`;
  const res = await fetch(path, { ...opts, headers });
  if (res.status === 401) {
    logout();
    throw new Error("Session expired, please sign in again");
  }
  if (!res.ok) {
    let message = `HTTP ${res.status}`;
    try {
      const data = await res.json();
      if (data.error) message = data.error;
    } catch {}
    throw new Error(message);
  }
  if (res.status === 204) return null;
  return res.json();
}

function showMessage(text, isError = false) {
  const el = $("#message");
  el.textContent = text;
  el.classList.toggle("error", isError);
  el.hidden = false;
}

function clearMessage() {
  $("#message").hidden = true;
}

function show(view) {
  $("#auth").hidden = view !== "auth";
  $("#app").hidden = view !== "app";
}

function setMode(mode) {
  state.mode = mode;
  $("#tab-login").classList.toggle("active", mode === "login");
  $("#tab-register").classList.toggle("active", mode === "register");
  $("#auth-submit").textContent = mode === "login" ? "Sign in" : "Register";
  $("#password").autocomplete = mode === "login" ? "current-password" : "new-password";
}

function logout() {
  state.token = null;
  state.email = null;
  localStorage.removeItem("token");
  localStorage.removeItem("email");
  show("auth");
}

function enterApp() {
  $("#user-email").textContent = state.email;
  setMonthRange(0);
  show("app");
  refresh();
}

function fmtDate(d) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function setMonthRange(offset) {
  const from = $("#from").value;
  const base = from
    ? new Date(Number(from.slice(0, 4)), Number(from.slice(5, 7)) - 1, 1)
    : new Date();
  const first = new Date(base.getFullYear(), base.getMonth() + offset, 1);
  const last = new Date(first.getFullYear(), first.getMonth() + 1, 0);
  $("#from").value = fmtDate(first);
  $("#to").value = fmtDate(last);
}

function filterParams() {
  const params = new URLSearchParams();
  const from = $("#from").value;
  const to = $("#to").value;
  if (from) params.set("from", from);
  if (to) params.set("to", to);
  return params;
}

const usd = (value) =>
  String(value).startsWith("-") ? `-$${String(value).slice(1)}` : `$${value}`;

async function loadSummary() {
  const params = filterParams();
  const query = params.toString();
  const data = await api(`/api/analytics/summary${query ? `?${query}` : ""}`);
  $("#sum-income").textContent = usd(data.total_income);
  $("#sum-expense").textContent = usd(data.total_expense);
  const net = Number(data.net);
  const netEl = $("#sum-net");
  netEl.textContent = usd(data.net);
  netEl.classList.toggle("neg", net < 0);
  netEl.classList.toggle("pos", net >= 0);
}

async function loadExpenses() {
  const params = filterParams();
  params.set("page", state.page);
  params.set("per_page", state.perPage);
  const data = await api(`/api/expenses?${params}`);
  state.total = data.total;
  state.page = data.page;
  state.expenses = data.expenses;

  if (state.total > 0 && data.expenses.length === 0 && state.page > 1) {
    state.page -= 1;
    return loadExpenses();
  }

  $("#rows").innerHTML = data.expenses
    .map(
      (e) => `<tr>
        <td>${esc(e.date)}</td>
        <td>${esc(e.account.replaceAll("_", " "))}</td>
        <td>${esc(e.category)}</td>
        <td class="desc">${esc(e.description)}</td>
        <td class="num ${e.kind === "income" ? "pos" : "neg"}">${esc(e.amount)} ${esc(e.currency)}</td>
        <td class="num">${e.amount_usd == null ? "–" : `$${esc(e.amount_usd)}`}</td>
        <td><button type="button" class="btn ghost row-details" data-id="${e.id}">Details</button></td>
      </tr>`
    )
    .join("");

  const pages = Math.max(1, Math.ceil(state.total / state.perPage));
  $("#page-info").textContent = `Page ${state.page} of ${pages}`;
  $("#prev").disabled = state.page <= 1;
  $("#next").disabled = state.page >= pages;
  $("#empty-hint").hidden = data.expenses.length !== 0;
}

async function refresh() {
  clearMessage();
  try {
    await Promise.all([loadSummary(), loadExpenses()]);
  } catch (err) {
    showMessage(err.message, true);
  }
}

function uploadReport(data) {
  if ("imported" in data) {
    return `CSV import: ${data.imported} new, ${data.duplicates} duplicates, ${data.receipts_attached} receipts attached, ${data.converted} USD-converted`;
  }
  const attached = data.expense_id ? "attached to an expense" : "pending (no matching expense yet)";
  return `Receipt ${data.status}: ${data.total} ${data.currency} on ${data.date}, ${data.item_count} items — ${attached}`;
}

$("#tab-login").addEventListener("click", () => setMode("login"));
$("#tab-register").addEventListener("click", () => setMode("register"));

$("#auth-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const body = JSON.stringify({
    email: $("#email").value.trim(),
    password: $("#password").value,
  });
  const path = state.mode === "login" ? "/api/auth/login" : "/api/auth/register";
  try {
    const data = await api(path, { method: "POST", body });
    state.token = data.token;
    state.email = data.user.email;
    localStorage.setItem("token", data.token);
    localStorage.setItem("email", data.user.email);
    state.page = 1;
    enterApp();
  } catch (err) {
    alert(err.message);
  }
});

$("#logout").addEventListener("click", logout);

$("#filters").addEventListener("submit", (event) => {
  event.preventDefault();
  state.page = 1;
  refresh();
});

function shiftMonth(offset) {
  setMonthRange(offset);
  state.page = 1;
  refresh();
}

$("#prev-month").addEventListener("click", () => shiftMonth(-1));
$("#next-month").addEventListener("click", () => shiftMonth(1));

$("#upload-form").addEventListener("change", async (event) => {
  const file = event.target.files[0];
  event.target.value = "";
  if (!file) return;
  const body = new FormData();
  body.append("file", file);
  try {
    const data = await api("/api/expenses/upload", { method: "POST", body });
    showMessage(uploadReport(data));
    await Promise.all([loadSummary(), loadExpenses()]);
  } catch (err) {
    showMessage(err.message, true);
  }
});

$("#rows").addEventListener("click", (event) => {
  const btn = event.target.closest(".row-details");
  if (btn) openDetails(btn.dataset.id);
});

$("#prev").addEventListener("click", () => {
  state.page -= 1;
  refresh();
});

$("#next").addEventListener("click", () => {
  state.page += 1;
  refresh();
});

function openDetails(id) {
  const expense = state.expenses.find((e) => e.id === id);
  if (!expense) return;
  resetModal();
  state.currentExpense = expense;
  renderDetail(expense);
  $("#details-modal").showModal();
  fetchReceipt(id);
}

function renderDetail(e) {
  const rows = [
    ["Date", esc(e.date)],
    ["Account", esc(e.account.replaceAll("_", " "))],
    ["Category", esc(e.category)],
    ["Kind", esc(e.kind)],
    ["Amount", `${esc(e.amount)} ${esc(e.currency)}`],
    ["USD", e.amount_usd == null ? "–" : `$${esc(e.amount_usd)}`],
    ["Description", esc(e.description)],
  ];
  $("#detail-fields").innerHTML = rows
    .map(([label, value]) => `<dt>${label}</dt><dd>${value}</dd>`)
    .join("");
}

async function fetchReceipt(id) {
  $("#receipt-block").textContent = "Loading receipt…";
  try {
    renderReceipt(await api(`/api/expenses/${id}/receipt`));
  } catch {
    $("#receipt-block").textContent = "No receipt attached";
  }
}

function renderReceipt(receipt) {
  const items = receipt.items
    .map(
      (item) => `<tr>
        <td>${esc(item.position)}</td>
        <td>${esc(item.name)}${Number(item.discount) !== 0 ? ` <span class="disc">−${esc(item.discount)}</span>` : ""}</td>
        <td class="num">${esc(item.quantity)} × ${esc(item.unit_price)}</td>
        <td class="num">${esc(item.total)}</td>
      </tr>`
    )
    .join("");
  $("#receipt-block").innerHTML = `
    <p class="receipt-total">${esc(receipt.total)} ${esc(receipt.currency)} — ${esc(receipt.fiscal_id)}</p>
    <table>
      <thead><tr><th>#</th><th>Item</th><th class="num">Qty × Price</th><th class="num">Total</th></tr></thead>
      <tbody>${items}</tbody>
    </table>`;
}

function prefillEdit(e) {
  $("#edit-date").value = e.date;
  $("#edit-account").value = e.account;
  $("#edit-kind").value = e.kind;
  $("#edit-category").value = e.category;
  $("#edit-amount").value = e.amount;
  $("#edit-currency").value = e.currency;
  $("#edit-description").value = e.description;
}

function resetModal() {
  state.currentExpense = null;
  $("#detail-view").hidden = false;
  $("#edit-form").hidden = true;
  $("#edit-error").hidden = true;
}

$("#modal-close").addEventListener("click", () => {
  $("#details-modal").close();
});

$("#modal-ok").addEventListener("click", () => {
  $("#details-modal").close();
});

$("#details-modal").addEventListener("close", resetModal);

$("#edit-expense").addEventListener("click", () => {
  prefillEdit(state.currentExpense);
  $("#detail-view").hidden = true;
  $("#edit-form").hidden = false;
});

$("#edit-cancel").addEventListener("click", () => {
  $("#edit-form").hidden = true;
  $("#edit-error").hidden = true;
  $("#detail-view").hidden = false;
});

$("#edit-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const body = JSON.stringify({
    date: $("#edit-date").value,
    account: $("#edit-account").value,
    category: $("#edit-category").value.trim(),
    amount: $("#edit-amount").value.trim(),
    kind: $("#edit-kind").value,
    currency: $("#edit-currency").value.trim(),
    description: $("#edit-description").value,
  });
  try {
    await api(`/api/expenses/${state.currentExpense.id}`, { method: "PUT", body });
    $("#details-modal").close();
    await Promise.all([loadSummary(), loadExpenses()]);
  } catch (err) {
    const errorEl = $("#edit-error");
    errorEl.textContent = err.message;
    errorEl.hidden = false;
  }
});

$("#delete-expense").addEventListener("click", async () => {
  if (!confirm("Delete this expense?")) return;
  try {
    await api(`/api/expenses/${state.currentExpense.id}`, { method: "DELETE" });
    $("#details-modal").close();
    await Promise.all([loadSummary(), loadExpenses()]);
  } catch (err) {
    $("#details-modal").close();
    showMessage(err.message, true);
  }
});

if (state.token) {
  enterApp();
} else {
  show("auth");
}
