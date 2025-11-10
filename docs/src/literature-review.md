# Literature Review

The following review outlines the key developments, standards, and technologies that inform the design of the Versioned Patient Repository (VPR), particularly with regard to data storage, patient ownership, and open-source architecture.

## Data Storage Models for patient records

### Database-centric models

Most traditional EPR systems are built using centralised relational databases. These models are well-established in clinical informatics and can scale effectively within single organisations. However, they often pose significant challenges when records need to move between systems, and are typically tightly coupled to the organisation’s software stack.

### File-based and version-controlled models

In contrast to centralised databases, file-based systems allow for portability, transparency, and simplified version control. Several notable efforts have explored this approach. We will explore them here.

Burstein ([2020a](#burstein-2020a) & [2020b](#burstein-2020b)) describes a proof-of-concept system for medical record-keeping based entirely on plain-text files and Git, developed for rural health centres in Rwanda where internet connectivity is unreliable. Instead of using a traditional database, the system stores patient data in human-readable YAML files and uses Git to manage version control, replication, and audit trails. This architecture prioritises offline resilience, transparency, and long-term accessibility, avoiding vendor lock-in and enabling data portability across devices. While not suitable for all settings, the project demonstrates that file-based, version-controlled health records can meet real clinical needs, especially in environments where simplicity, traceability, and decentralisation are key.

Adams ([2020a](#adams-2020a) & [2020b](#adams-2020b)) presents a lightweight system called Hugo Clinic Notes, designed for smaller clinics and written in Markdown. The tool organises patient notes by name, date, and appointment time, supports multiple note types (such as assessments and follow-ups with embedded media), and includes a printable view so records can be easily saved or shared. While patient data itself is not version controlled, Git is used to manage the form templates and archetypes, allowing clinical structures to evolve safely over time. Notes are edited manually as Markdown files outside the system, and Hugo is then used to regenerate the site as a set of static HTML pages. Emphasising portability, simplicity, and clinician or patient control over data location, the project demonstrates how static site generation and file-based structures can support clinical documentation when traditional EPR systems may be unnecessarily complex. Although primarily used and maintained by its creator, it remains a useful example of how low-dependency, open tooling can be adapted for healthcare use.

Wack et al. ([2025](#wack-2025)) describe the gitOmmix approach for clinical omics data, which integrates version‑control systems (specifically Git and git‑annex) with provenance knowledge‑graphs (based on PROV‑O) to enhance clinical data warehouses. The authors argue that traditional CDWs (clinical data warehouses) lack robust support for large data files and longitudinal provenance tracking. In response, gitOmmix uses Git to version and track large files (via git‑annex) and aligns version history with a provenance graph so that each data analysis, decision, and patient sample can be traced back comprehensively. The system supports querying the relationships between raw files, analyses, and clinical outcomes by combining versioning metadata and provenance semantics. Although the work is tailored particularly to omics (genomics, pathology, radiology) rather than general EPRs, it provides a compelling file‑based, version‑aware model for health‑data systems and thus offers a useful precedent for the VPR’s versioned and patient‑centric architecture.

### Blockchain

Reen et al. ([2019](#reen-2019)) propose a decentralised e‑health record management system that combines blockchain technology with the InterPlanetary File System (IPFS) to give patients control of their health‑data flows. The architecture stores encrypted patient records on IPFS and uses smart contracts on a blockchain to manage access authorisations, thereby enabling patient‑centric sharing, auditability and privacy. While the system emphasises distributed storage and peer‑to‑peer data exchange rather than a central database, the authors note trade‑offs in terms of scalability and the maturity of supporting infrastructure. The work provides an instructive example of how versioning, audit trails and patient‑owned data constructs can be applied in health‑care settings and hence offers relevant insight for the design of the VPR.

Shi et al. (2020) conduct a systematic literature review of blockchain applications in electronic health‑record (EHR) systems, specifically assessing how such architectures tackle security and privacy challenges. They identify that while blockchain introduces transparency, immutability and decentralised control, its implementation in healthcare faces major hurdles in scalability, interoperability, and compliance with regulatory requirements. The study thereby underscores both the promise and the limitations of distributed‑ledger approaches for patient data management and highlights the viability of hybrid or alternate version‑controlled architectures — making it a relevant reference point when considering the design of the VPR.

Antwi et al. ([2021](#antwi-2021)) explore how Hyperledger Fabric, a private blockchain system, could be used to manage electronic health records securely. They set up a series of test cases that mimic real clinical use, including patient and clinician access permissions, data privacy controls and how different types of files such as X-rays are handled. The study found that Hyperledger Fabric worked well for keeping data confidential and traceable, but struggled with large-scale storage and the legal requirement to delete data completely. The authors suggest that while it is not a perfect solution, private blockchains like Fabric could form part of future systems that let patients control access to their records while maintaining a strong audit trail.

Kumari et al. ([2024](#kumari-2024)) describe HealthRec-Chain, a system designed to give patients greater control over their health data while keeping it secure and shareable. The approach combines two technologies: blockchain, to record who accesses information, and IPFS, a distributed file system used to store the medical files themselves. Each record is automatically encrypted before being stored, and patients can grant or remove access through simple permissions. The authors test the system’s performance and find that this hybrid model could offer a practical balance between security, transparency, and scalability—avoiding some of the heavy costs of traditional blockchain-only designs.

## Patient focused systems

Fasten-OnPrem, is an open-source, self-hosted application for personal or family electronic medical records that aims to bring disparate data from many clinics, labs, and insurers into one place under the individual’s control. The record system was built by Kulantuga and Szilagyi ([2025](#kulantuga-2025)) and sponsored by Fasten Health. Fasten-OnPrem supports key standards such as FHIR and OAuth2 so users can link their existing records rather than manually scanning everything. The system is designed for non-clinical settings (families rather than hospitals), but demonstrates how file-based, patient-owned aggregation of health records can work in practice—emphasising portability, transparency and user control rather than heavy institutional infrastructure.

## Healthcare Data Standards

This section surveys structural and exchange standards commonly used to represent and move electronic patient records.

### Structural models

#### openEHR

A specification for modelling clinical content using archetypes and templates. It separates clinical knowledge from data persistence and can be serialised as JSON or XML. It includes constructs for composition, context and audit history.

#### HL7 CDA

A document-centric model for clinical correspondence and reports. CDA defines a structured container with narrative text and coded elements, typically exchanged as XML.

### Exchange and APIs

#### HL7 v2

A widely deployed messaging standard for admissions, transfers, results and orders. It is compact and event-driven, and remains prevalent in secondary care integrations.

#### HL7 FHIR

A resource-based standard designed for web APIs. It serialises naturally to JSON or XML, supports profiles to constrain use, and provides resources for provenance, consent and audit. FHIR is now the dominant choice for modern interfaces and patient-facing apps.

#### IHE Profiles

Integration profiles such as XDS and MHD specify how documents and resources are published, discovered and retrieved across organisations, building on CDA and FHIR.

## Open Source in Healthcare

The literature describes open source as a credible approach in digital health when paired with clear governance and resourcing. Reported benefits include transparency of code and data models, which supports independent security assessment, clinical safety review and reproducibility. Reuse is a second theme: open components can be adapted to local workflows, shortening time to deliver standard capabilities such as FHIR APIs, document rendering and integration gateways. Several studies note that open interfaces create incentives for interoperability by lowering switching costs across vendors and sites. Cost is presented more cautiously. Licence fees may fall, particularly for infrastructure, yet staffing, integration and long-term support remain material and require realistic budgets.

Sustainability depends on governance. Successful programmes set explicit licensing strategies, contribution guidelines and release cadences, and treat clinical safety artefacts as first-class versioned assets alongside source code. Open projects do not remove the need for security engineering. Threat modelling, coordinated vulnerability disclosure, continuous testing and dependency management are still required, and are often easier to scrutinise when build pipelines are public.

Case studies illustrate these points in practice. OpenEyes shows that a specialty EPR can be developed in the open and operated across several NHS Trusts with formal safety processes. OpenSAFELY demonstrates that transparent code and specifications can coexist with strict controls on patient data, enabling reproducible analytics at scale. OpenPrescribing provides public methods and code for prescribing analyses, supporting peer scrutiny and iterative improvement. Internationally, OpenMRS and GNU Health show long-running community models for longitudinal records and public health, while the openEHR community maintains shared archetypes and templates that allow vendors to converge on common clinical content.

Risks are also highlighted. Fragmentation can occur if forks diverge without stewardship, hidden costs can surface during integration and migration, and security can be misunderstood if openness is taken as a substitute for active assurance. The reported mitigations are straightforward but non-trivial: maintainers who curate contributions, product ownership with clinical sponsorship, published roadmaps, funded support arrangements and independent security testing. Overall, the evidence supports open source as a practical route to transparency, reuse and safer interoperability, provided it is treated as a long-term programme with disciplined governance rather than a short-term cost-saving exercise.

---

## References

<span id="adams-2020a"></span>
Adams, J. (2020a). 'Hugo Clinic Notes Theme'. Available at:
[https://jmablog.com/post/hugo-clinic-notes/](https://jmablog.com/post/hugo-clinic-notes/) (Accessed: 5 Nov. 2025).

<span id="adams-2020b"></span>
Adams, J. (2020b). 'Hugo Clinic Notes'. GitHub repository. Available at: [https://github.com/jmablog/hugo-clinic-notes](https://github.com/jmablog/hugo-clinic-notes) (Accessed: 5 Nov. 2025).

<span id="antwi-2021"></span>
Antwi, M., Adnane, A., Ahmad, F., Hussain, R., Habib ur Rehman M. and Kerrache, C.A. (2021). 'The case of HyperLedger Fabric as a blockchain solution for healthcare applications', *Blockchain: Research and Applications*, 2 (1), pp. 1-15, doi: [https://doi.org/10.1016/j.bcra.2021.100012](https://doi.org/10.1016/j.bcra.2021.100012).

<span id="burstein-2020a"></span>  
Burstein, A. (2020a). 'Improving Health Care with Plain-Text Medical Records and Git'. Available at: [https://www.gizra.com/content/plain-text-medical-records/](https://www.gizra.com/content/plain-text-medical-records/) (Accessed: 5 Nov. 2025).

<span id="burstein-2020b"></span>  
Burstein, A. (2020b). 'mdr-git'. Github repository. Available at: [https://github.com/amitaibu/mdr-git](https://github.com/amitaibu/mdr-git) (Accessed: 5 Nov. 2025).

<span id="kulantuga-2025"></span>
Kulantuga, J. and Szilagyi, A (2025). 'fasten-onprem', GitHub repository. Available at: [https://github.com/fastenhealth/fasten-onprem](https://github.com/fastenhealth/fasten-onprem) (Accessed: 10 Nov. 2025).

<span id="kumari-2024"></span>
Kumari, D., Parmar, A.S., Goyal, H.S., Mishra, K. and Panda S. (2024). 'HealthRec-Chain: Patient-centric blockchain enabled IPFS for privacy preserving scalable health data', *Computer Networks*, 241, p. 110223, doi: [https://doi.org/10.1016/j.comnet.2024.110223](https://doi.org/10.1016/j.comnet.2024.110223).

<span id="reen-2019"></span>
Reen, G. S., Mohandas, M. and Venkaresan S. (2019). 'Decentralized Patient Centric e-Health Record Management System
using Blockchain and IPFS', *IEEE*. Available at:[https://arxiv.org/pdf/2009.14285](https://arxiv.org/pdf/2009.14285) (Accessed: 6 Nov. 2025).

<span id="shi-2020"></span>
Shi S., He, D., Li, L., Khan N., Khan, M. K. and Choo, K-K. R. (2020). 'Applications of blockchain in ensuring the security and privacy of electronic health record systems: A survey', *Computers & Security*, 97, pp. 1-20. doi: [https://doi.org/10.1016/j.cose.2020.101966](https://doi.org/10.1016/j.cose.2020.101966).

<span id="wack-2025"></span>
Wack, M., Coulet, A., Burgun, A. and Bastien, R. (2025). 'Enhancing clinical data warehousing with provenance data to support longitudinal analyses and large file management: The gitOmmix approach for genomic and image data', *Journal of Biomedical Informatics*, 193, p. 104788, doi: [https://doi.org/10.1016/j.jbi.2025.104788](https://doi.org/10.1016/j.jbi.2025.104788) (Accessed: 5 Nov. 2025).
