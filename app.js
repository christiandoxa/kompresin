const $ = (id) => document.getElementById(id);

function fmtBytes(n) {
    const u = ["B", "KB", "MB", "GB"];
    let i = 0, x = n;
    while (x >= 1024 && i < u.length - 1) {
        x /= 1024;
        i++;
    }
    return `${x.toFixed(i === 0 ? 0 : 2)} ${u[i]}`;
}

function clamp(n, lo, hi) {
    return Math.min(hi, Math.max(lo, n));
}

function extOf(file) {
    const name = (file?.name || "").toLowerCase();
    const m = name.match(/\.([a-z0-9]+)$/);
    return m ? m[1] : "";
}

function kindOf(file) {
    const mime = (file?.type || "").toLowerCase();
    const ext = extOf(file);
    if (mime.includes("pdf") || ext === "pdf") return "pdf";
    if (mime.includes("png") || ext === "png") return "png";
    if (mime.includes("jpg") || mime.includes("jpeg") || ext === "jpg" || ext === "jpeg") return "jpeg";
    return "unknown";
}

async function fileToArrayBuffer(file) {
    return await file.arrayBuffer();
}

async function compressFile(file, wasm) {
    const preset = Number($("preset").value);
    const quality = Number($("quality").value);
    const maxSide = Number($("maxSide").value);

    const outModeSel = $("outMode").value;
    const pngMode = $("pngMode").value;
    const colors = Number($("colors").value);
    const dithering = $("dither").checked;
    const forceQuant = $("forceQuant").checked;

    const bgValue = $("bg").value;
    const wantsTransparent = bgValue === "transparent";
    const bg = wantsTransparent ? [255, 255, 255] : bgValue.split(",").map(Number);
    const bg_r = bg[0] | 0, bg_g = bg[1] | 0, bg_b = bg[2] | 0;

    const ab = await fileToArrayBuffer(file);
    const result = wasm.compress_file(
        new Uint8Array(ab),
        file.type || "",
        extOf(file),
        outModeSel,
        clamp(quality, 1, 100),
        clamp(preset, 0, 2),
        Math.max(0, maxSide | 0),
        bg_r,
        bg_g,
        bg_b,
        pngMode,
        clamp(colors, 1, 256),
        dithering,
        forceQuant,
        wantsTransparent
    );

    const outMode = result.outMode;
    const bytes = result.bytes;
    const base = file.name.replace(/\.(png|jpg|jpeg|pdf)$/i, "");
    const name = outMode === "pdf"
        ? file.name.replace(/\.pdf$/i, "") + "-compressed.pdf"
        : base + (outMode === "png" ? ".png" : ".jpg");
    const mime = outMode === "pdf"
        ? "application/pdf"
        : (outMode === "png" ? "image/png" : "image/jpeg");

    return {bytes, mime, name};
}

function renderResult(el, fileLike) {
    el.innerHTML = "";
    const div = document.createElement("div");
    div.innerHTML = `
    <div><strong>Name:</strong> ${fileLike.name}</div>
    <div><strong>Size:</strong> ${fmtBytes(fileLike.size)}</div>
    ${fileLike.type ? `<div><strong>Type:</strong> ${fileLike.type}</div>` : ""}
  `;
    el.appendChild(div);
}

let currentOutUrl = null;
let currentBeforeUrl = null;

function clearObjectUrl(url) {
    if (url) URL.revokeObjectURL(url);
}

function resetCompare(compareWrap, compareBefore, compareAfter) {
    clearObjectUrl(currentBeforeUrl);
    currentBeforeUrl = null;
    compareBefore.removeAttribute("src");
    compareAfter.removeAttribute("src");
    compareWrap.hidden = true;
}

function renderAfter(el, out, original, compareEls) {
    el.innerHTML = "";
    const size = out.bytes.length ?? out.bytes.byteLength ?? 0;
    const blob = new Blob([out.bytes], {type: out.mime});
    clearObjectUrl(currentOutUrl);
    currentOutUrl = URL.createObjectURL(blob);
    const url = currentOutUrl;

    const ratio = original.size ? (size / original.size) : 0;
    const pct = original.size ? ((1 - ratio) * 100) : 0;

    const a = document.createElement("a");
    a.href = url;
    a.download = out.name;
    a.className = "btn";
    a.textContent = `Download (${fmtBytes(size)} | saved ${pct.toFixed(1)}%)`;

    const info = document.createElement("div");
    info.style.marginTop = "8px";
    info.innerHTML = `<div><strong>Size:</strong> ${fmtBytes(size)}</div>`;

    el.appendChild(a);
    el.appendChild(info);

    const {compareWrap, compareBefore, compareAfter, compareStage} = compareEls;
    const showCompare = original.type.startsWith("image/") && out.mime.startsWith("image/");
    if (!showCompare) {
        resetCompare(compareWrap, compareBefore, compareAfter);
        return;
    }

    clearObjectUrl(currentBeforeUrl);
    currentBeforeUrl = URL.createObjectURL(original);
    compareBefore.src = currentBeforeUrl;
    compareAfter.src = url;
    compareStage.style.setProperty("--compare", "50%");
    compareStage.style.aspectRatio = "";
    compareWrap.hidden = false;
}

window.addEventListener("TrunkApplicationStarted", () => {
    const wasm = window.wasmBindings;
    const themeToggle = $("themeToggle");
    const statusWrap = $("statusWrap");
    const progressBar = $("progressBar");
    const compareWrap = $("compareWrap");
    const compareBefore = $("compareBefore");
    const compareAfter = $("compareAfter");
    const compareStage = $("compareStage");
    const compareLine = $("compareLine");

    const fileInput = $("file");
    const runBtn = $("run");
    const status = $("status");
    const q = $("quality");
    const qv = $("qv");

    const syncPngFromLevel = () => {
        if ($("pngMode").value !== "auto") return;
        const level = Number(q.value);
        const colorsFromLevel = Math.round(8 + (level / 100) * 248);
        $("colors").value = String(clamp(colorsFromLevel, 1, 256));
    };

    q.addEventListener("input", () => {
        qv.textContent = q.value;
        syncPngFromLevel();
    });

    $("pngMode").addEventListener("change", () => {
        syncPngFromLevel();
    });

    const applyTheme = (theme) => {
        document.documentElement.setAttribute("data-theme", theme);
        themeToggle.textContent = theme === "dark" ? "Light mode" : "Dark mode";
    };

    const setProgress = (percent) => {
        if (percent === null) {
            progressBar.style.width = "0%";
            statusWrap.querySelector(".progress").style.display = "none";
            return;
        }
        statusWrap.querySelector(".progress").style.display = "block";
        progressBar.style.width = `${Math.max(0, Math.min(100, percent))}%`;
    };

    const storedTheme = localStorage.getItem("theme");
    const preferredDark = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
    const initialTheme = storedTheme || (preferredDark ? "dark" : "light");
    applyTheme(initialTheme);

    const advancedPanel = $("advancedPanel");
    const pngPanel = $("pngPanel");
    const updateAdvancedVisibility = (file) => {
        const kind = kindOf(file);
        const showAdvanced = kind === "png" || kind === "jpeg";
        advancedPanel.hidden = !showAdvanced;
        pngPanel.hidden = kind !== "png";
        if (!showAdvanced) advancedPanel.open = false;
        if (kind !== "png") pngPanel.open = false;
    };

    themeToggle.addEventListener("click", () => {
        const current = document.documentElement.getAttribute("data-theme") || "light";
        const next = current === "dark" ? "light" : "dark";
        localStorage.setItem("theme", next);
        applyTheme(next);
    });

    fileInput.addEventListener("change", () => {
        const f = fileInput.files?.[0];
        runBtn.disabled = !f;
        status.textContent = f ? "Ready to compress." : "";
        if (f) renderResult($("before"), f);
        $("after").innerHTML = "";
        setProgress(null);
        updateAdvancedVisibility(f);
        clearObjectUrl(currentOutUrl);
        currentOutUrl = null;
        resetCompare(compareWrap, compareBefore, compareAfter);
    });

    updateAdvancedVisibility(fileInput.files?.[0]);
    const updateCompareAspect = (img) => {
        const w = img.naturalWidth;
        const h = img.naturalHeight;
        if (w > 0 && h > 0) {
            compareStage.style.aspectRatio = `${w} / ${h}`;
        }
    };
    compareBefore.addEventListener("load", () => updateCompareAspect(compareBefore));
    compareAfter.addEventListener("load", () => updateCompareAspect(compareAfter));
    const setCompareFromClientX = (clientX) => {
        const rect = compareStage.getBoundingClientRect();
        if (!rect.width) return;
        const x = Math.max(0, Math.min(rect.width, clientX - rect.left));
        const pct = (x / rect.width) * 100;
        compareStage.style.setProperty("--compare", `${pct}%`);
    };
    compareStage.addEventListener("pointerdown", (e) => {
        setCompareFromClientX(e.clientX);
        compareStage.setPointerCapture(e.pointerId);
    });
    compareStage.addEventListener("pointermove", (e) => {
        if (compareStage.hasPointerCapture(e.pointerId)) {
            setCompareFromClientX(e.clientX);
        }
    });
    compareStage.addEventListener("pointerup", (e) => {
        compareStage.releasePointerCapture(e.pointerId);
    });
    compareLine.addEventListener("pointerdown", (e) => {
        setCompareFromClientX(e.clientX);
        compareStage.setPointerCapture(e.pointerId);
        e.preventDefault();
    });

    runBtn.addEventListener("click", async () => {
        const f = fileInput.files?.[0];
        if (!f) return;

        runBtn.disabled = true;
        status.textContent = "Processing…";
        setProgress(5);

        try {
            setProgress(15);
            const out = await compressFile(f, wasm);

            setProgress(100);
            status.textContent = "Done.";
            renderAfter($("after"), out, f, {
                compareWrap,
                compareBefore,
                compareAfter,
                compareStage,
            });
        } catch (e) {
            console.error(e);
            status.textContent = "Failed: " + (e?.message || e);
        } finally {
            setTimeout(() => setProgress(null), 600);
            runBtn.disabled = false;
        }
    });
});
