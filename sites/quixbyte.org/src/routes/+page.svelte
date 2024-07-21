<script lang="ts">
    import Fa from "svelte-fa";
    import {
        faServer,
        faCode,
        faGaugeHigh,
        faDownload,
        faBook,
    } from "@fortawesome/free-solid-svg-icons";
    import { faDiscord, faGithub } from "@fortawesome/free-brands-svg-icons";
</script>

<svelte:head>
    <title>QuixByte - Sync with ease</title>
</svelte:head>

<section class="hero">
    <img src="/quixbyte_full.svg" alt="QuixByte" />
    <p>
        QuixByte is an opensource file service, which allows users to quickly
        synchronize and backup their files across multiple devices.
    </p>
    <ul>
        <li>
            <a href="https://github.com/qb-rs/qb" class="button"
                ><Fa icon={faGithub} />&nbsp; Github</a
            >
        </li>
        <li>
            <a href="https://discord.gg/Cq2UpzeTnm" class="button"
                ><Fa icon={faDiscord} />&nbsp; Discord</a
            >
        </li>
    </ul>
</section>

<hr />

<section>
    <span>&gt; batteries included</span>
    <h1>Works out-of-the-box</h1>

    <p>
        QuixByte can synchronize to Google Drive, Dropbox and more
        without installing anything else or doing hacky things.
    </p>

    <ul>
        <li>
            <a href="#" class="button"
                ><Fa icon={faDownload} />&nbsp; Installation Guide</a
            >
        </li>
    </ul>
</section>

<hr />

<section>
    <span>&gt; bleeding edge tech stack</span>
    <h1>Performance to an extreme</h1>

    <p>
        QuixByte is written completely in rust, which gives it
        the power and safety to withstand your needs.
    </p>

    <ul>
        <li>
            <a
                href="#"
                class="button"><Fa icon={faGaugeHigh} />&nbsp; Inner Workings</a
            >
        </li>
    </ul>
</section>

<hr />

<section>
    <span>&gt; your data - your server</span>
    <h1>Selfhostable with ease</h1>

    <p>
        Running QuixByte on your server is a breeze thanks to our ecosystem.
        Hosting a custom server allows not having to use external services
        like Google Drive or Dropbox.
    </p>

    <ul>
        <li>
            <a href="#" class="button"
                ><Fa icon={faServer} />&nbsp; Setup Guide</a
            >
        </li>
    </ul>
</section>

<hr />

<section>
    <span>&gt; completely opensource</span>
    <h1>Built by the community</h1>

    <p>
        QuixByte is licensed under a <a class="link" href="https://github.com/qb-rs/qb/blob/main/LICENSE">copyleft license</a>,
        which means that its source code is free to change and play around with.
        We happily accept contributions.
    </p>

    <ul>
        <li>
            <a
                href="#"
                class="button"><Fa icon={faBook} />&nbsp; Contributor's Guide</a
            >
        </li>
        <li>
            <a
                href="#"
                class="button"><Fa icon={faCode} />&nbsp; Source Code</a
            >
        </li>
    </ul>
</section>

<hr />

<footer>
    <p>
        This page is under active development.
        <a href="https://github.com/qb-rs/qb/issues" class="link"
            >Report issue</a
        >
    </p>

    <p>
        &copy; Copyright 2024 The QuixByte Authors |
        <a href="https://github.com/qb-rs/qb/blob/main/LICENSE" class="link"
            >LICENSE</a
        >
    </p>
</footer>

<style>
    .hero img {
        height: 6rem;
        font-family: "Teko", sans-serif;
        font-size: 5rem;
        margin-bottom: 20px;
    }

    .hero {
        margin-top: 4rem;
    }

    span {
        display: inline-block;
        font-size: 32px;
        font-family: "Handjet", cursive;
        color: blue;
        margin-bottom: 0.3em;
    }

    section {
        margin-top: 8rem;
        margin-bottom: 8rem;
    }

    section p {
        font-size: 20px;
        max-width: 560px;
    }

    section ul,
    .hero ul {
        display: flex;
        flex-wrap: wrap;
        list-style: none;
        column-gap: 10px;
        row-gap: 5px;

        margin-top: 40px;
    }

    footer {
        padding-top: 3rem;
        font-size: 14px;
    }

    @media (max-width: 480px) {
        span {
            font-size: 34px;
        }

        section h1 {
            font-size: 56px;
        }

        section p {
            font-size: 18px;
        }

        .button {
            font-size: 18px;
        }

        .hero img {
            max-width: 100%;
        }

        footer {
            font-size: 12px;
            text-align: center;
        }
    }

    hr {
        border: none;
        border-top: 1px solid rgba(0, 0, 0, 0.1);
    }
</style>
