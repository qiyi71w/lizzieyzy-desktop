// Java KataGoAutoSetupDialog.createWeightsSection / createWeightCatalogBlock.
import type { Model, ResourceItem, ResourcesView } from "../contract/generated";
import { ask } from "../dialogs";
import { product, t } from "../i18n";
import type { SetupCard, SetupContext } from "./card";
import { currentTarget, packageTarget, planSetup, resourceCommand, resourceView, setupVerified } from "./dialog";
import { action, actions, future, node, notice, resourceText, section, size } from "./setupText";

interface WeightRow {
  id: string;
  name: string;
  transformer: boolean;
  elo: string;
  date: string;
  bytes: number | bigint | null;
  path: string | null;
  item: ResourceItem | null;
  url: string;
  model: Model | null;
}
export function weightsCard(_ctx: SetupContext): SetupCard {
  const element = node("div", "", "setup-weights");
  let tab: "official" | "custom" = "official", selected = "", last: ResourcesView | null = null;
  const render = () => {
    const r = resourceView(); if (!r) return;
    last = r; element.replaceChildren();
    element.append(node("h2", t("AutoSetup.weightsTitle")), node("p", t("AutoSetup.weightsSubtitle"), "setup-hint"));
    const current = node("div", "", "setup-current-weight"); const stone = node("span", "", "setup-stones");
    const description = node("div"); description.append(node("small", t("AutoSetup.currentWeight")), node("strong", r.currentSetup.modelName ?? t("AutoSetup.notFound")));
    current.append(stone, description, node("span", r.currentSetup.weight ? t("AutoSetup.currentlyUsingShort") : t("AutoSetup.notReady"), "setup-use-badge"));
    element.append(current, notice());
    const target = currentTarget() ?? r.inventory.packages.filter((p) => p.usable).map(packageTarget).find((p) => p !== null) ?? null;
    const installed = target && r.inventory.packages.find((p) => p.executable === target.executable);
    const version = installed?.receipt.version ?? (target?.executable === r.currentSetup.executable ? r.currentSetup.engineVersion : r.verification?.version);
    const versionParts = version?.replace(/^v/, "").split(".").map(Number);
    const supportsTransformer = !!versionParts && (versionParts[0] > 1 || versionParts[0] === 1 && versionParts[1] >= 17);
    const recommendations = node("div", "", "setup-recommendations");
    const labels: Record<string, string> = { balanced: t("AutoSetup.recommendationBalanced"), strongest: t("AutoSetup.recommendationStrongest"), lightweight: t("AutoSetup.recommendationLightweight") };
    for (const recommendation of r.recommendation?.models ?? []) {
      const model = r.models.find((m) => m.id === recommendation.id); if (!model) continue;
      const card = node("article", "", "setup-recommendation");
      const button = action(`setup.weight.recommend.${model.id}`, "", () => { tab = "official"; selected = model.id; render(); });
      button.className = "setup-recommendation-select"; button.append(node("strong", labels[recommendation.tier] ?? model.displayFamily), node("small", resourceText(recommendation.hintKey)), node("small", `${model.displayFamily} · ${size(model.sizeBytes)} · ${model.publishedAt}`));
      const weight = r.inventory.weights.find((w) => w.catalogModelId === model.id);
      const inUse = !!r.currentSetup.weight && (weight?.path === r.currentSetup.weight || r.currentSetup.modelName === model.fileName.replace(/\.(bin|txt)\.gz$/i, ""));
      const incompatible = model.architecture === "transformer" && !supportsTransformer;
      const use = action(`setup.weight.recommend.${model.id}.action`, inUse ? t("AutoSetup.currentlyUsingShort") : weight ? incompatible ? t("AutoSetup.transformerRequires117Short") : t("AutoSetup.useWeight") : t("AutoSetup.downloadShort"), async () => {
        if (weight && target) await planSetup(target, weight.path);
        else if (!weight && await ask(t("AutoSetup.title"), node("p", product("setupDownloadConfirm", model.displayFamily, size(model.sizeBytes))), t("AutoSetup.downloadShort"), product("cancel"), true)) await resourceCommand({ type: "Download", item: { type: "Model", model: model.id } });
      }, !inUse && (!weight || setupVerified(target) && !incompatible));
      if (weight && incompatible) use.title = t("AutoSetup.transformerRequires117");
      card.append(button, use); card.classList.toggle("selected", selected === model.id); recommendations.append(card);
    }
    element.append(recommendations, node("h3", t("AutoSetup.moreWeights")));
    const catalog = section(""), tabs = node("div", "", "setup-weight-tabs");
    for (const [id, title] of [["official", t("AutoSetup.officialWeightTab")], ["custom", t("AutoSetup.customWeightTab")]] as const) {
      const button = action(`setup.weight.tab.${id}`, title, () => { tab = id; selected = ""; render(); }); button.classList.toggle("selected", id === tab); tabs.append(button);
    }
    const reload = action("setup.weight.reload", "↻", () => tab === "official" ? resourceCommand({ type: "RefreshRemoteWeights" }) : resourceCommand({ type: "Inspect", refresh: true })); reload.title = t("AutoSetup.refreshOfficialWeights"); reload.setAttribute("aria-label", t("AutoSetup.refreshOfficialWeights")); tabs.append(reload); catalog.append(tabs);
    const rows: WeightRow[] = [];
    if (tab === "official") {
      const seen = new Set<string>();
      for (const model of r.models) {
        const name = model.fileName.replace(/\.(bin|txt)\.gz$/i, ""); seen.add(name);
        const installed = r.inventory.weights.find((w) => w.catalogModelId === model.id || w.modelName === name);
        const remote = r.onlineWeights.find((w) => w.name === name);
        rows.push({ id: model.id, name: model.displayFamily, transformer: model.architecture === "transformer", elo: remote?.elo ?? "—", date: model.publishedAt, bytes: model.sizeBytes, path: installed?.path ?? null, item: { type: "Model", model: model.id }, url: r.modelUrls[model.id] ?? model.downloadUrl, model });
      }
      for (const remote of r.onlineWeights) {
        if (seen.has(remote.name)) continue; seen.add(remote.name);
        const installed = r.inventory.weights.find((w) => w.modelName === remote.name || w.path.endsWith(`${remote.name}.bin.gz`));
        rows.push({ id: remote.name, name: remote.name, transformer: remote.transformer, elo: remote.elo || "—", date: remote.publishedAt, bytes: remote.sizeBytes, path: installed?.path ?? null, item: { type: "OnlineWeight", name: remote.name }, url: remote.downloadUrl, model: null });
      }
    } else {
      for (const w of r.inventory.weights.filter((w) => !w.catalogModelId)) {
        const name = w.modelName ?? w.path.split(/[\\/]/).at(-1) ?? w.path;
        rows.push({ id: w.path, name, transformer: /tflrs|^kata1-tf[23]-b\d+c\d+-/i.test(name), elo: "—", date: "—", bytes: w.size, path: w.path, item: null, url: "", model: null });
      }
    }
    if (!rows.some((w) => w.id === selected)) selected = rows.find((w) => w.path === r.currentSetup.weight)?.id ?? rows[0]?.id ?? "";
    const tableWrap = node("div", "", "setup-weight-table-wrap"), table = node("table", "", "setup-weight-table"), head = node("thead"), header = node("tr");
    for (const label of [t("AutoSetup.weightColumnModel"), t("AutoSetup.transformerBadge"), t("AutoSetup.weightColumnElo"), t("AutoSetup.weightColumnReleaseDate"), t("AutoSetup.weightColumnStatus")]) header.append(node("th", label));
    head.append(header); table.append(head); const tbody = node("tbody");
    for (const w of rows) {
      const tr = node("tr"); tr.classList.toggle("selected", w.id === selected);
      const model = node("td"); const select = action(`setup.weight.select.${w.id}`, w.name, () => { selected = w.id; render(); }); select.title = w.url || w.path || w.name; model.append(select);
      const badge = node("td"); badge.append(node("span", w.transformer ? "TF" : "CNN", "setup-architecture"));
      const status = w.path === r.currentSetup.weight ? t("AutoSetup.currentlyUsingShort") : w.path ? t("AutoSetup.downloaded") : t("AutoSetup.notDownloaded");
      tr.append(model, badge, node("td", w.elo), node("td", w.date.slice(0, 10)), node("td", status)); tbody.append(tr);
    }
    table.append(tbody); tableWrap.append(table); catalog.append(tableWrap);
    if (!rows.length) catalog.append(node("p", tab === "custom" ? t("AutoSetup.noCustomWeights") : t("AutoSetup.noRemoteWeights"), "setup-hint"));
    const chosen = rows.find((w) => w.id === selected), details = node("div", "", "setup-weight-details");
    const info = node("div"); info.append(node("strong", chosen?.name ?? t("AutoSetup.notFound")));
    if (chosen) {
      info.title = chosen.url || chosen.path || "";
      info.append(node("p", `${chosen.transformer ? t("AutoSetup.transformerBadge") : "CNN"} · Elo ${chosen.elo} · ${size(chosen.bytes)} · ${chosen.date}`, "setup-hint"));
      if (chosen.model) info.append(node("small", `${product("setupLicense")}: ${chosen.model.licenseSummary} · ${chosen.model.licenseTexts.join(", ")}\n${product("setupSource")}: ${chosen.url}`));
    }
    details.append(info);
    const blocked = !!chosen?.transformer && !supportsTransformer;
    if (blocked) info.append(node("p", t("AutoSetup.transformerRequires117"), "setup-warning"));
    actions(details,
      action("setup.weight.download", t("AutoSetup.downloadShort"), async () => {
        if (!chosen?.item) return;
        if (await ask(t("AutoSetup.title"), node("p", product("setupDownloadConfirm", chosen.name, size(chosen.bytes))), t("AutoSetup.downloadShort"), product("cancel"), true)) await resourceCommand({ type: "Download", item: chosen.item });
      }, !!chosen?.item && !chosen.path),
      action("setup.weight.use", chosen?.path === r.currentSetup.weight ? t("AutoSetup.currentlyUsingShort") : t("AutoSetup.useWeight"), () => target && chosen?.path ? planSetup(target, chosen.path) : undefined, setupVerified(target) && !!chosen?.path && chosen.path !== r.currentSetup.weight && !blocked),
      action("setup.weight.import", t("AutoSetup.importWeight"), () => resourceCommand({ type: "ImportWeight", path: null })));
    details.querySelector<HTMLButtonElement>("[data-key='setup.weight.import']")!.title = t("AutoSetup.importWeightFilter");
    catalog.append(details);
    if (r.remoteStatus === "refreshing") catalog.append(node("p", t("AutoSetup.loadingRemote"), "setup-hint"));
    else if (r.remoteStatus.startsWith("AutoSetup.")) catalog.append(node("p", resourceText(r.remoteStatus), "setup-warning"));
    element.append(catalog);
    const quick = section(t("AutoSetup.quickAnalysisModel"), t("AutoSetup.quickAnalysisModelDescription")); actions(quick, future("setup.quick.download", t("AutoSetup.downloadOnDemand"))); quick.append(node("small", product("unavailable"))); element.append(quick);
    const human = section(t("AutoSetup.humanSlModel"), t("AutoSetup.humanSlModelDescription")); actions(human, future("setup.humansl.download", t("AutoSetup.downloadOnDemand")), future("setup.humansl.import", t("AutoSetup.importWeight"))); human.append(node("small", product("unavailable"))); element.append(human);
  };
  return { element, update() { if (resourceView() !== last) render(); } };
}
