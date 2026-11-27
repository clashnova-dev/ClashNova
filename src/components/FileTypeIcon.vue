<script setup lang="ts">
  import { computed } from "vue";
  import { Icon, type IconifyIcon } from "@iconify/vue";
  // 彩色文件类型图标：vscode-icons（按用到的图标单独导入，tree-shake，不再手绘）。
  import defaultFile from "@iconify-icons/vscode-icons/default-file";
  import markdown from "@iconify-icons/vscode-icons/file-type-markdown";
  import pdf from "@iconify-icons/vscode-icons/file-type-pdf2";
  import word from "@iconify-icons/vscode-icons/file-type-word";
  import excel from "@iconify-icons/vscode-icons/file-type-excel";
  import csv from "@iconify-icons/vscode-icons/file-type-csv";
  import powerpoint from "@iconify-icons/vscode-icons/file-type-powerpoint";
  import zip from "@iconify-icons/vscode-icons/file-type-zip";
  import image from "@iconify-icons/vscode-icons/file-type-image";
  import svg from "@iconify-icons/vscode-icons/file-type-svg";
  import audio from "@iconify-icons/vscode-icons/file-type-audio";
  import video from "@iconify-icons/vscode-icons/file-type-video";
  import js from "@iconify-icons/vscode-icons/file-type-js";
  import ts from "@iconify-icons/vscode-icons/file-type-typescript";
  import python from "@iconify-icons/vscode-icons/file-type-python";
  import go from "@iconify-icons/vscode-icons/file-type-go";
  import rust from "@iconify-icons/vscode-icons/file-type-rust";
  import java from "@iconify-icons/vscode-icons/file-type-java";
  import html from "@iconify-icons/vscode-icons/file-type-html";
  import css from "@iconify-icons/vscode-icons/file-type-css";
  import json from "@iconify-icons/vscode-icons/file-type-json";
  import yaml from "@iconify-icons/vscode-icons/file-type-yaml";
  import shell from "@iconify-icons/vscode-icons/file-type-shell";
  import sql from "@iconify-icons/vscode-icons/file-type-sql";
  import text from "@iconify-icons/vscode-icons/file-type-text";

  const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 28 });

  const BY_EXT: Record<string, IconifyIcon> = {
    md: markdown,
    pdf,
    doc: word,
    docx: word,
    odt: word,
    rtf: word,
    pages: word,
    xls: excel,
    xlsx: excel,
    ods: excel,
    numbers: excel,
    csv,
    ppt: powerpoint,
    pptx: powerpoint,
    odp: powerpoint,
    key: powerpoint,
    zip,
    rar: zip,
    "7z": zip,
    gz: zip,
    tar: zip,
    bz2: zip,
    xz: zip,
    png: image,
    jpg: image,
    jpeg: image,
    gif: image,
    webp: image,
    heic: image,
    avif: image,
    bmp: image,
    tiff: image,
    svg,
    mp3: audio,
    wav: audio,
    flac: audio,
    aac: audio,
    m4a: audio,
    ogg: audio,
    mp4: video,
    mov: video,
    mkv: video,
    webm: video,
    avi: video,
    m4v: video,
    js,
    jsx: js,
    mjs: js,
    cjs: js,
    ts,
    tsx: ts,
    py: python,
    go,
    rs: rust,
    java,
    html,
    htm: html,
    css,
    json,
    yaml,
    yml: yaml,
    sh: shell,
    bash: shell,
    zsh: shell,
    sql,
    txt: text,
    log: text,
    ini: text,
    conf: text,
  };

  const icon = computed(() => {
    const base = props.name.split(/[\\/]/).pop() || "";
    const dot = base.lastIndexOf(".");
    const ext = dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
    return BY_EXT[ext] || defaultFile;
  });
</script>
<template>
  <Icon :icon="icon" class="file-type-icon" :width="size" :height="size" aria-hidden="true" />
</template>
<style lang="scss" scoped>
  .file-type-icon {
    display: block;
    flex: none;
  }
</style>
