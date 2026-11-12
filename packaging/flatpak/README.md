# Empacotamento Flatpak & Submissão para o Flathub

Este diretório contém o manifesto completo para compilar o **AlgoritmosLab** (`io.github.imarcosvinicius.AlgoritmosLab`) **100% offline a partir do código-fonte**, atendendo às regras do **Flathub**.

## Arquivos

- `io.github.imarcosvinicius.AlgoritmosLab.yml`: manifesto principal (`org.gnome.Platform` `50`, `rust-stable`, `node24`).
- `io.github.imarcosvinicius.AlgoritmosLab.desktop`: lançador Linux compatível com `desktop-file-validate`.
- `io.github.imarcosvinicius.AlgoritmosLab.metainfo.xml`: metadados AppStream para lojas gráficas (GNOME Software, KDE Discover, Flathub).
- `cargo-sources.json` e `node-sources.json`: dependências offline geradas a partir de `src-tauri/Cargo.lock` e `package-lock.json`.
- `update-sources.sh`: script para regenerar os dois `.json` sempre que dependências forem atualizadas.

---

## Como Gerar o `.flatpak` via GitHub Actions (Qualquer Distro)

1. Faça `git push` para o repositório no GitHub.
2. Publique uma tag de versão (ex.: `git tag v0.2.0 && git push origin v0.2.0`) **ou** dispare manualmente **Actions → Release (Build Multiplataforma e Flatpak) → Run workflow**.
3. O arquivo `AlgoritmosLab-<versão>-x86_64.flatpak` (junto com `.deb`, `.AppImage`, `.exe`, `.msi` e `.dmg`) será anexado na **Release** (ou nos artefatos do workflow).
4. Em qualquer distribuição Linux com Flatpak instalado:
   ```bash
   flatpak install --user AlgoritmosLab-*.flatpak
   flatpak run io.github.imarcosvinicius.AlgoritmosLab
   ```

---

## Como Publicar Oficialmente no Flathub

1. **Capturar e versionar ao menos 1 screenshot no repositório:**
   O Flathub exige ao menos uma captura de tela (`<screenshots>`) no `.metainfo.xml`. Tire um print da IDE em funcionamento, salve por exemplo em `docs/screenshots/ide-principal.png`, faça commit e adicione ao `io.github.imarcosvinicius.AlgoritmosLab.metainfo.xml` antes de abrir o PR no Flathub:
   ```xml
   <screenshots>
     <screenshot type="default">
       <caption>Editor Portugol, depurador passo a passo e console interativo no AlgoritmosLab</caption>
       <image>https://raw.githubusercontent.com/imarcosvinicius/algoritmos-lab/main/docs/screenshots/ide-principal.png</image>
     </screenshot>
   </screenshots>
   ```
   Valide localmente sem erros/avisos com:
   ```bash
   desktop-file-validate packaging/flatpak/io.github.imarcosvinicius.AlgoritmosLab.desktop
   appstreamcli validate --pedantic --explain packaging/flatpak/io.github.imarcosvinicius.AlgoritmosLab.metainfo.xml
   ```

2. **Fazer fork e Pull Request em `flathub/flathub`:**
   - Faça fork de <https://github.com/flathub/flathub> e crie uma branch a partir da branch `new-pr`:
     ```bash
     git clone --branch=new-pr https://github.com/<SEU_USUARIO>/flathub.git
     cd flathub
     git checkout -b add-io.github.imarcosvinicius.AlgoritmosLab
     ```
   - Copie `io.github.imarcosvinicius.AlgoritmosLab.yml`, `cargo-sources.json` e `node-sources.json` para a raiz do PR no `flathub` (e troque a fonte local `type: dir` pela tag git pública):
     ```yaml
     sources:
       - type: git
         url: https://github.com/imarcosvinicius/algoritmos-lab.git
         tag: v0.2.0
         commit: <HASH_COMPLETO_DO_COMMIT_DA_TAG>
       - node-sources.json
       - cargo-sources.json
     ```
   - **Nota sobre `--filesystem=home`:** se o revisor do Flathub perguntar o motivo, explique que o comando educacional `arquivo "dados.txt"` do Portugol acessa arquivos auxiliares na mesma pasta do `.alg` aberto pelo estudante, exigindo acesso ao diretório além de um único arquivo via portal.

3. **Verificar no portal do Flathub:**
   Após aprovação e criação do repositório `flathub/io.github.imarcosvinicius.AlgoritmosLab`, faça login em <https://flathub.org> com sua conta GitHub `imarcosvinicius` para concluir a verificação oficial com o selo verificado.
