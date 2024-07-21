<!-- SPDX-License-Identifier: AGPL-3.0-only -->

<!-- ████████████████████████████████████████████████ -->
<!-- █─▄▄▄─█▄─██─▄█▄─▄█▄─▀─▄█▄─▄─▀█▄─█─▄█─▄─▄─█▄─▄▄─█ -->
<!-- █─██▀─██─██─███─███▀─▀███─▄─▀██▄─▄████─████─▄█▀█ -->
<!-- ▀───▄▄▀▀▄▄▄▄▀▀▄▄▄▀▄▄█▄▄▀▄▄▄▄▀▀▀▄▄▄▀▀▀▄▄▄▀▀▄▄▄▄▄▀ -->
<!-- https://github.com/QuixByte/qb/blob/main/LICENSE -->

<!-- (c) Copyright 2023 The QuixByte Authors -->

<script lang="ts">
  import "../app.css";

  import "@fontsource/teko/latin-700.css";
  import "@fontsource/inter/latin-400.css";
</script>

<main>
  <slot />
</main>

<style>
  @import url("https://fonts.googleapis.com/css2?family=Handjet:wght@400&display=swap");

  main {
    max-width: 1000px;
    width: 85%;
    margin: auto;
    padding: 4rem 0rem;
  }
</style>
