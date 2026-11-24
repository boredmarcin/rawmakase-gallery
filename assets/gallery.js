(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("gallery-data").textContent);
  const $ = (selector) => document.querySelector(selector);
  const large = (photo) => "photos/large/" + encodeURIComponent(photo.name);
  const thumb = (photo) => "photos/thumb/" + encodeURIComponent(photo.name);
  const heartIcon = '<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20.5s-7.5-4.6-7.5-10.2A4.3 4.3 0 0 1 12 7.6a4.3 4.3 0 0 1 7.5 2.7c0 5.6-7.5 10.2-7.5 10.2z"/></svg>';
  const downloadIcon = '<svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v11m-5-5l5 5 5-5M5 20h14"/></svg>';

  // Favorites, kept in this browser for this gallery, by each photo's id
  // (the catalog's UUID), or its file name without one.
  const id = (photo) => photo.id || photo.name;
  const key = "gallery-favorites-" + data.id;
  let favorites;
  try { favorites = new Set(JSON.parse(localStorage.getItem(key) || "[]")); } catch { favorites = new Set(); }
  const save = () => { try { localStorage.setItem(key, JSON.stringify([...favorites])); } catch { /* private mode */ } };

  let showing = "all";
  const shown = () => data.photos.filter((p) => showing === "all" || favorites.has(id(p)));

  // Text.
  $(".cover-title").textContent = data.title;
  $(".cover-subtitle").textContent = data.subtitle;
  $(".cover-brand").textContent = data.author;
  $(".bar-title").textContent = data.title;
  $(".footer-author").textContent = data.author;
  $(".count-all").textContent = data.photos.length;

  const photos = $("#photos");
  photos.classList.add("layout-" + data.layout);

  function tile(photo) {
    const item = document.createElement("figure");
    item.className = "tile";
    item.style.margin = "0";
    const img = document.createElement("img");
    img.src = data.layout === "column" ? large(photo) : thumb(photo);
    img.width = photo.width;
    img.height = photo.height;
    img.alt = photo.name;
    img.loading = "lazy";
    img.decoding = "async";
    img.className = "is-loading";
    img.addEventListener("load", () => img.classList.remove("is-loading"), { once: true });
    const actions = document.createElement("div");
    actions.className = "tile-actions";
    const heart = document.createElement("button");
    heart.type = "button";
    heart.className = "heart";
    heart.dataset.id = id(photo);
    heart.innerHTML = heartIcon;
    heart.addEventListener("click", (e) => { e.stopPropagation(); toggle(id(photo)); });
    const download = document.createElement("a");
    download.href = large(photo);
    download.download = photo.name;
    download.setAttribute("aria-label", "Download " + photo.name);
    download.innerHTML = downloadIcon;
    download.addEventListener("click", (e) => e.stopPropagation());
    actions.append(heart, download);
    item.append(img, actions);
    item.tabIndex = 0;
    item.addEventListener("click", () => open(photo));
    item.addEventListener("keydown", (e) => { if (e.key === "Enter") open(photo); });
    return item;
  }

  // Masonry fills columns left to right, each photo into the shortest one.
  function columnsFor(width) {
    if (width < 640) return 2;
    if (width < 1000) return 3;
    if (width < 1500) return 4;
    return 5;
  }
  let columns = 0;
  function render() {
    const list = shown();
    photos.replaceChildren();
    $(".empty").hidden = list.length > 0;
    if (data.layout === "masonry") {
      columns = columnsFor(photos.clientWidth);
      const cols = Array.from({ length: columns }, () => {
        const col = document.createElement("div");
        col.className = "column";
        photos.append(col);
        return { col, height: 0 };
      });
      for (const photo of list) {
        const target = cols.reduce((a, b) => (b.height < a.height ? b : a));
        target.col.append(tile(photo));
        target.height += photo.height / photo.width;
      }
    } else {
      for (const photo of list) photos.append(tile(photo));
    }
    paintHearts();
  }
  window.addEventListener("resize", () => {
    if (data.layout === "masonry" && columnsFor(photos.clientWidth) !== columns) render();
  });

  function paintHearts() {
    for (const heart of document.querySelectorAll(".heart[data-id]")) {
      const on = favorites.has(heart.dataset.id);
      heart.classList.toggle("is-on", on);
      heart.setAttribute("aria-pressed", String(on));
      heart.setAttribute("aria-label", on ? "Remove from favorites" : "Add to favorites");
    }
    $(".count-favorites").textContent = favorites.size;
    $(".send").hidden = favorites.size === 0;
  }
  function toggle(key) {
    if (favorites.has(key)) favorites.delete(key); else favorites.add(key);
    save();
    if (showing === "favorites" && viewer.hidden) render(); else paintHearts();
  }

  for (const tab of document.querySelectorAll(".tab")) {
    tab.addEventListener("click", () => {
      showing = tab.dataset.show;
      for (const t of document.querySelectorAll(".tab")) t.classList.toggle("is-on", t === tab);
      render();
      photos.scrollIntoView({ block: "start" });
    });
  }

  // Viewer.
  const viewer = $(".viewer");
  const viewerPhoto = document.createElement("img");
  viewerPhoto.className = "viewer-photo";
  viewerPhoto.alt = "";
  $(".viewer-stage").append(viewerPhoto);
  const viewerHeart = $(".viewer-heart");
  let current = 0;
  let list = [];
  let opener = null;
  function show(i) {
    current = (i + list.length) % list.length;
    const photo = list[current];
    viewerPhoto.classList.add("is-loading");
    viewerPhoto.onload = () => viewerPhoto.classList.remove("is-loading");
    viewerPhoto.src = large(photo);
    viewerPhoto.alt = photo.name;
    viewerHeart.dataset.id = id(photo);
    $(".viewer-download").href = large(photo);
    $(".viewer-download").download = photo.name;
    $(".viewer-caption").textContent = photo.name;
    paintHearts();
    for (const n of [current - 1, current + 1]) {
      const next = list[(n + list.length) % list.length];
      if (next) new Image().src = large(next);
    }
  }
  function open(photo) {
    list = shown();
    opener = document.activeElement;
    viewer.hidden = false;
    document.body.style.overflow = "hidden";
    show(list.indexOf(photo));
    // The dialog itself takes the focus, so no button shows a focus ring on opening.
    viewer.focus({ preventScroll: true });
  }
  // Slideshow: the next photo every few seconds, until stopped or closed.
  const play = $(".viewer-play");
  let slideshow = null;
  function stopSlideshow() {
    clearInterval(slideshow);
    slideshow = null;
    play.classList.remove("is-on");
    play.setAttribute("aria-pressed", "false");
    play.setAttribute("aria-label", "Start slideshow");
  }
  play.addEventListener("click", () => {
    if (slideshow) { stopSlideshow(); return; }
    slideshow = setInterval(() => show(current + 1), 4000);
    play.classList.add("is-on");
    play.setAttribute("aria-pressed", "true");
    play.setAttribute("aria-label", "Stop slideshow");
  });
  function close() {
    stopSlideshow();
    viewer.hidden = true;
    document.body.style.overflow = "";
    if (showing === "favorites") render();
    if (opener) opener.focus();
  }
  $(".viewer-close").addEventListener("click", close);
  $(".viewer-prev").addEventListener("click", () => show(current - 1));
  $(".viewer-next").addEventListener("click", () => show(current + 1));
  viewerHeart.addEventListener("click", () => toggle(id(list[current])));
  document.addEventListener("keydown", (e) => {
    if (!$(".sheet").hidden && e.key === "Escape") { $(".sheet").hidden = true; return; }
    if (viewer.hidden) return;
    if (e.key === "Escape") close();
    else if (e.key === " ") { e.preventDefault(); play.click(); }
    else if (e.key === "ArrowLeft") show(current - 1);
    else if (e.key === "ArrowRight") show(current + 1);
    else if (e.key === "f" || e.key === "F") toggle(id(list[current]));
  });
  let touch = null;
  viewer.addEventListener("touchstart", (e) => { touch = e.touches[0]; }, { passive: true });
  viewer.addEventListener("touchend", (e) => {
    if (!touch) return;
    const dx = e.changedTouches[0].clientX - touch.clientX;
    const dy = e.changedTouches[0].clientY - touch.clientY;
    touch = null;
    if (Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy)) show(current + (dx < 0 ? 1 : -1));
    else if (dy > 90) close();
  });

  // Sending favorites.
  const sheet = $(".sheet");
  $(".send").addEventListener("click", () => {
    const names = data.photos.filter((p) => favorites.has(id(p))).map((p) => p.name);
    const text = names.join("\n");
    $(".sheet-list").value = text;
    const mail = $(".sheet-mail");
    mail.hidden = !data.email;
    if (data.email) {
      mail.href = "mailto:" + encodeURIComponent(data.email) + "?subject=" +
        encodeURIComponent("Favorites: " + data.title) + "&body=" + encodeURIComponent(text);
    }
    sheet.hidden = false;
    $(".sheet-copy").focus();
  });
  $(".sheet-close").addEventListener("click", () => { sheet.hidden = true; });
  sheet.addEventListener("click", (e) => { if (e.target === sheet) sheet.hidden = true; });
  $(".sheet-copy").addEventListener("click", async () => {
    const button = $(".sheet-copy");
    try {
      await navigator.clipboard.writeText($(".sheet-list").value);
    } catch {
      $(".sheet-list").select();
      document.execCommand("copy");
    }
    button.textContent = "Copied";
    setTimeout(() => { button.textContent = "Copy list"; }, 1600);
  });

  render();
})();
