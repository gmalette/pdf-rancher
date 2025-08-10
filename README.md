# PDF Rancher

PDF Rancher is a versatile application designed for merging and splicing PDF files. While macOS users can rely on Preview for these tasks, Windows users often lack a comparable native solution. Although web services are available, they pose significant concerns regarding data sovereignty. PDF Rancher addresses this issue by providing a secure, offline tool for managing your PDF files. Unlike Adobe and Foxit, which charge a subscription at extortionary rates, PDF Rancher is completely free.

![PDF Rancher Screenshot](/doc/screenshot-1.png)

## Comparison Table

| Feature                    | PDF Rancher | macOS Preview | Foxit & Adobe PDF Editor | Online PDF Services | PDF Reader | LibreOffice Draw |
|----------------------------|-------------|---------------|--------------------------|---------------------|------------|------------------|
| Free                       | ✅          | ✅            | ❌                       | ✅                  | ✅         | ✅               |
| Data sovereignty           | ✅          | ✅            | ✅                       | ❌                  | ✅         | ✅               |
| Merge and splice documents | ✅          | ✅            | ✅                       | ✅                  | ❌         | ✅               |
| Modify content             | ❌          | ✅            | ✅                       | ✅                  | ❌         | ✅               |
| Mixed Page Layout          | ✅          | ✅            | ✅                       | ✅                  | ✅         | ❌               |

## Build and Publish

1.  **Update PDFium Binaries (Optional)**

    If you need to update the bundled PDFium binaries to the latest version, run:

    ```bash
    cargo run -p xtask -- update-pdfium
    ```

2.  **Create a Release**

    To start the release process, run:

    ```bash
    cargo run -p xtask -- release
    ```

    This command will:
    - Generate and check the third-party license file.
    - Bump the package version.
    - Build the application for macOS (ARM64) and Windows (x64 and ARM64).
    - Commit, tag, and push the changes.
    - Create a draft release on GitHub with the built artifacts.

3.  **Publish the Release**

    After the script finishes, go to the releases page of the GitHub repository, review the draft release, and publish it.

## Contributing

We welcome contributions from the community! Here are some ways you can contribute:

1. **Report Bugs**: If you find a bug, please report it by opening an issue on GitHub.
2. **Suggest Features**: If you have an idea for a new feature, please open an issue to discuss it.
3. **Submit Pull Requests**: If you want to contribute code, fork the repository and submit a pull request with your changes.
4. **Improve Documentation**: Help us improve our documentation by making it clearer and more comprehensive.

### Getting Started

1. Fork the repository on GitHub.
2. Clone your forked repository to your local machine.
3. Create a new branch for your feature or bug fix.
4. Make your changes and commit them with clear and concise commit messages.
5. Push your changes to your forked repository.
6. Open a pull request on the main repository.

### Code of Conduct

Please note that this project is released with a [Contributor Code of Conduct](CODE_OF_CONDUCT.md). By participating in this project, you agree to abide by its terms.

### License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for more details.
