<div align="center">

# Minimal SQL

Um cliente SQL para o desktop.<br>
Aberto, gratuito e sem chave de licença.

[Baixar](https://github.com/PedroSad1/minimal-sql/releases) · [GPLv3](LICENSE.md)

</div>

---

O Minimal SQL mantém várias conexões abertas ao mesmo tempo. A barra lateral mostra bancos, schemas e tabelas. Um duplo clique abre a tabela em uma aba. Uma query fica no editor, e o resultado aparece abaixo.

A interface é escura, com pouco contraste e com a fonte IBM Plex Sans.

## Bancos

PostgreSQL, CockroachDB, Amazon Redshift, GreengageDB, MySQL, MariaDB, TiDB, StarRocks, SQLite, SQL Server, Redis e Google BigQuery.

## Uso

- Cada conexão guarda as próprias abas. Tabelas com o mesmo nome mostram o nome da conexão.
- O filtro da tabela aceita `=`, `!=`, `in`, `like`, `>`, `>=`, `<`, `<=`, `is null` e `is not null`. O `AND` vira `OR` com um clique.
- A grade carrega uma página de cada vez. O scroll pede a página seguinte.
- A seleção copia como TSV. O menu da coluna também copia JSON, Markdown e SQL.
- Um valor JSON abre em um painel ao lado.
- A estrutura da tabela pode ser editada. O app pede confirmação antes de salvar.

## Instalar

Baixe o arquivo do seu sistema em [Releases](https://github.com/PedroSad1/minimal-sql/releases). Abra o arquivo. O terminal não entra nesse passo.

| Sistema | Arquivo | Passo |
| --- | --- | --- |
| macOS | `.dmg` | Abra o arquivo e arraste o app para Applications |
| Windows | `.exe` | Abra o instalador |
| Linux | `.deb` ou AppImage | Abra o `.deb` no Ubuntu ou no Debian, ou abra o AppImage |

A primeira abertura pode mostrar um aviso do sistema. Esse aviso permanece até o pacote ter um certificado da Apple e um certificado do Windows.

## Desenvolvimento

Rust 1.98+ e Node 24+.

```bash
cd apps/desktop
pnpm install
pnpm tauri dev
```

Os testes de SQLite rodam direto:

```bash
cargo test -p graphite-core -p graphite-drivers -p graphite-appdb
```

PostgreSQL, MySQL, Redis e SQL Server usam Docker:

```bash
docker compose -f docker-compose.test.yml up -d
GRAPHITE_DOCKER=1 cargo test -p graphite-drivers -- --ignored --nocapture
```

O app usa Tauri 2, Vue 3 e Rust. Os crates internos continuam com o prefixo `graphite-`.

## Licença

[GNU GPLv3](LICENSE.md).

O Minimal SQL toma ideias do Beekeeper Studio Community. Ele não é o Beekeeper Studio. Este repositório não inclui o código comercial daquele projeto. Todas as funções do Minimal SQL são gratuitas.
