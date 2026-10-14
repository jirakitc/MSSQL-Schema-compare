# Schema Compare

โปรแกรม Windows สำหรับเปรียบเทียบ **SQL Server schema** เขียนด้วย Rust มี native GUI และ CLI ไม่มีระบบ license, activation หรือ licensing server ตัว comparison engine ทำงานอิสระจาก Redgate

เวอร์ชัน **1.0.0** ใช้ UX/UI ของ SQL Compare 14 เป็นแนวทาง: toolbar, หน้าต่าง Edit project, Source → Target strip, results แบบแบ่งกลุ่ม และ SQL Differences สองฝั่งที่จัดบรรทัดและ scroll พร้อมกัน ไม่แจก binary ของ Redgate และยังไม่ครอบคลุมทุกความสามารถของ SQL Compare 14

## Release 1.0.0

- จัดเมนูหลักแบบ SQL Data Compare: Project (New/Open/Save/Edit/Recent), Compare (Compare/Refresh/Results/Ignore/Filter/Swap), Deployment (Review SQL/Deploy) และเพิ่ม Export/Help สำหรับ report, snapshot และข้อมูลโปรแกรม; toolbar เหลือคำสั่งที่ใช้บ่อย
- Recent มีช่อง Search ค้นหา Source/Target address, database หรือ login แบบไม่สนตัวพิมพ์เล็ก/ใหญ่; กรองรายการภายในกลุ่มและเปิดกลุ่มที่พบให้เห็นทันที พร้อมปุ่ม × ล้างข้อความค้นหาโดยไม่ลบ history
- Recent connections แบ่งกลุ่มตามคู่ Source Address → Target Address (ไม่แยกตามชื่อ database/login) โดยกลุ่มและรายการล่าสุดอยู่ด้านบน; ยุบ/ขยายแต่ละกลุ่มได้ และ Use connection เลือกคู่ database ของแถวนั้น
- หน้า Start a comparison มี Recent connections (Source/Target 12 คู่ล่าสุดที่ Compare สำเร็จ) พร้อมปุ่ม Use connection และ Clear; เก็บข้ามการเปิดโปรแกรมใน `%LOCALAPPDATA%\SchemaCompare\recent-connections.json` โดยไม่บันทึก password; Clear ล้างเฉพาะรายการ ไม่เปลี่ยน project ปัจจุบัน
- Different / Source only / Target only เป็น multi-select status filters: เปลี่ยน filter แล้วเลือกเฉพาะแถวที่ตรงกับ status + Search objects + object types และยกเลิกแถวที่ถูกซ่อน; ปุ่ม Select filtered ใช้กฎเดียวกัน; Identical แสดงได้แต่ไม่เลือก Deploy
- เปลี่ยนข้อความฝั่ง Target ที่ไม่มี object เป็น **Not exists**
- ยึดคอลัมน์ฝั่ง Source สำหรับตารางที่เลือก: ALTER COLUMN รวมการลดขนาด/เปลี่ยน datatype และ NULL → NOT NULL โดยตรวจข้อมูลจริงก่อน; ลบคอลัมน์ที่มีเฉพาะ Target พร้อมปรับ dependency ตาม Source ใน transaction เดียวกัน
- การลด datatype ตรวจ TRY_CONVERT และ binary round-trip เพื่อป้องกัน truncation/rounding: หากข้อมูลไม่พอดี จะ fail และ rollback; DROP COLUMN ลบข้อมูลในคอลัมน์นั้น และชื่อที่ต่างกันถือเป็น drop/add โดยไม่คัดลอกค่าอัตโนมัติ; schema-bound/computed dependencies และ standalone user-created statistics ยังต้อง review
- แก้ Find Database: คลิกและพิมพ์ค้นหาใน dropdown ได้โดยไม่ปิด popup; ปิดเมื่อเลือกชื่อฐาน, คลิกด้านนอก หรือกด Escape

- Windows x64 optimized Release build พร้อมไอคอนและ FileVersion/ProductVersion 1.0.0 ใน EXE
- เวอร์ชันใน GUI และ CLI อ่านจาก Cargo package version เพื่อให้ตรงกัน

## ความสามารถจาก 0.6.4

- ไอคอน Schema Compare ออกแบบใหม่: ฐานข้อมูล Source สี cyan และ Target สีขาว พร้อมลูกศรเปรียบเทียบสองทางบนพื้น navy ใช้ใน EXE, title bar/taskbar และ header ของแอป
- ICO มีขนาด 16/24/32/48/64/128/256 และมีต้นฉบับ SVG/PNG ใน assets สร้างซ้ำได้ด้วย `tools/generate_icon.py` + Pillow; Windows build ฝัง icon resources ด้วย windres (GNU) หรือ rc.exe (MSVC)
- มี `cleanup-old-builds.ps1` สำหรับรันเองเพื่อลบเฉพาะ build รุ่นก่อน 1.0.0 ใน dist โดยตรวจ resolved paths และเก็บรุ่นล่าสุด/รุ่นใหม่กว่าไว้ การลบอัตโนมัติใน session นี้ถูกเครื่องมือปฏิเสธ จึงยังคงมี build เก่าอยู่

## ความสามารถจาก 0.6.3

- ตรวจ schema drift ตาม object key แทนลำดับ array จาก catalog และเทียบ dependencies เป็นชุด จึงไม่แจ้ง Fail จากการเรียง object/edges ใหม่ แต่ยังตรวจลำดับคอลัมน์, SQL definition, SET options และ schema metadata จริงทุก field
- Capture เรียง object ด้วย key ที่รวม parent เพื่อให้ index ชื่อซ้ำในคนละ Table มีลำดับแน่นอน
- หาก schema เปลี่ยนจริง ข้อความ Fail ระบุ Added/Removed/Modified, ชื่อ object และ field ที่เปลี่ยน โดยแสดงไม่เกิน 12 object พร้อมจำนวนที่เหลือ

## ความสามารถจาก 0.6.2

- แก้ cursor บนชื่อ Table/object เป็นลูกศรปกติ ปิดการเลือกข้อความภายในตาราง results และให้ชื่อ object รับคลิกเลือกแถวโดยตรง

## ความสามารถจาก 0.6.1

- เอาปุ่ม **Import .scp** ออกจาก toolbar

## ความสามารถจาก 0.6.0

- Source และ Target connect/read schema พร้อมกันใน background threads แยกกัน รวมทั้ง Compare, Deploy preflight และการ refresh หลัง Deploy; ไม่มีการต่อคิวอ่าน Source แล้วค่อย Target และแสดง status ของทั้งสองฝั่งแยกกัน
- หลัง Deploy แสดงหน้าต่าง **Deployment Success / Deployment Fail** พร้อมรายละเอียดและปุ่ม **Back to Compare** ผล Compare ใหม่อยู่ด้านหลังทันทีและเลือกแท็บ SQL Differences อัตโนมัติ
- หาก Deploy ล้มเหลว โปรแกรมโหลด Source/Target ใหม่เพื่อแสดง schema ปัจจุบันโดยไม่ retry deployment หาก refresh ไม่ได้ จะระบุว่าผลเดิมยังแสดงอยู่และปิด Deploy จนกว่า Compare จะสำเร็จ
- เอาปุ่ม **Deployment script...** ออกจากแถวปุ่มหน้า Compare แล้ว

## ความสามารถจาก 0.5.0


- เอาปุ่ม **Use open Redgate** ออกจาก toolbar, File menu และหน้าเริ่มต้น เหลือ Import .scp และการโหลด saved project เมื่อเปิดโปรแกรม
- ระหว่าง Compare แสดง status กลางหน้าจอพร้อมเวลาที่ใช้: Source/Target connect + read catalogs, decode objects, validate dependencies และ matching objects; ระหว่าง Deploy แสดง preflight, executing และ verification
- เพิ่ม **Deploy...** ที่หน้า Compare และ **Deploy to Target...** ในแท็บสคริปต์ หน้าต่าง Deploy แสดง server/database, จำนวน object/actions และสคริปต์ที่จะรัน กด Deploy to Target เพื่อเริ่มรันจริง
- Deploy ต้องมี live Target และไม่มี planner blockers ตรวจ Source และ Target ว่ายังตรงกับ snapshot ที่ review ไว้ก่อนรัน หาก schema หรือ server/database เปลี่ยนจะหยุดให้ Compare ใหม่ เมื่อรันสำเร็จจะอ่าน Target ซ้ำและตรวจ selected changes ก่อนแสดงผลใหม่
- รัน SQLCMD แบบ background ด้วย transaction, XACT_ABORT และ ON ERROR EXIT; หาก command ผิดพลาดจะไม่รัน batch ถัดไป และ connection ที่ปิดจะ rollback transaction ที่ยังไม่ commit รหัสผ่านอยู่ใน process environment ไม่อยู่ใน command line/project

## ความสามารถจากรุ่นก่อน


- หน้า Compare เริ่มด้วย **Type of object**: Table, Procedure, View, Function, Trigger และชนิดอื่น ๆ; รายการที่ต่างแสดงก่อน และมี **Show identical** สำหรับแสดง object ที่เหมือนกัน สามารถสลับ Group by เป็น Type of difference / No groups ได้
- **Edit project** ขนาดเริ่มต้นประมาณ 800×460 points, จำกัดความกว้างให้กะทัดรัด, ลดขนาด control และลาก title bar เพื่อย้าย dialog ได้ ตำแหน่งจะไม่เด้งกลับกึ่งกลาง
- **Database names** เป็น dropdown select เต็มความกว้าง โหลดรายการเมื่อเปิด dropdown เท่านั้น ไม่มีปุ่ม Fetch หรือ auto-fetch หลังพิมพ์ รองรับค้นหาชื่อฐานและรักษาค่าที่เลือกไว้เมื่อโหลดไม่สำเร็จ ปิดแล้วเปิดใหม่เพื่อโหลดซ้ำได้ Windows Authentication ไม่ต้องกรอก SQL password; SQL Authentication ต้องมี Username/Password รายการแสดงเฉพาะฐาน online ที่ login เข้าถึงได้ โดย query ผ่าน master
- Edit project มีเพียง **Data sources** และ **Ignore options**; เอาแท็บ Redgate import ออกแล้ว ตัวนำเข้าบน toolbar ยังใช้งานได้
- **หนึ่ง Table = หนึ่งแถว** รวม PK, UNIQUE, FK, CHECK, indexes และ DML triggers ภายใน Table; สถานะและ SQL Differences รวมองค์ประกอบด้วย เมื่อเลือก Table planner จะเลือกเฉพาะองค์ประกอบที่ต่าง แล้วจัดลำดับ dependency ก่อนสร้าง script ทั้ง GUI และ CLI ใช้ logic เดียวกัน
- เทียบ index INCLUDE columns เป็นชุดโดยไม่สนใจลำดับ แต่ยังตรวจลำดับ index keys; module headers ใช้ชื่อ schema/object จาก catalog จึงไม่แจ้งความต่างจาก CREATE/ALTER หรือชื่อแบบมี/ไม่มี schema ใน header
- Capture ตัด SQL Server diagram support objects ที่มี extended property `microsoft_database_tools_support` ออก รวมองค์ประกอบของตารางนั้น
- นำเข้า Ignore triggers, fill factor/padding, collation, comments, module SET, column name case และ constraints ที่รองรับจาก `.scp`; ไม่มี ForceColumnOrder จะเทียบ column metadata ตามชื่อ ไม่ถือการสลับตำแหน่งเป็นความต่าง
- รองรับ Synonym เป็นแถวอิสระ พร้อมสร้าง/แทนที่/ลบใน deployment script โดย target-only ยังคงต้องอนุญาต destructive drops
- จำนวน object ทั้งหมดยังอาจต่างจาก Redgate เพราะขอบเขต engine ยังไม่รวม users/roles/permissions และชนิดขั้นสูงอื่น ๆ ตามข้อจำกัดด้านล่าง ไม่ได้เพิ่มแถวจำลองให้ยอดตรง

### Ignore options

ตั้งค่าใน **Edit project → Ignore options** และบันทึกลง native project JSON ได้ เมื่อเปลี่ยนค่าจะคำนวณ comparison ใหม่จาก snapshot ที่โหลดไว้ทันที

| หมวด | Options |
|---|---|
| SQL text & module settings | Ignore SQL comments; Ignore ANSI_NULLS / QUOTED_IDENTIFIER |
| Tables & columns | Ignore column name case; Ignore column order; Ignore column collation; Ignore DEFAULT constraint names |
| Indexes, constraints & triggers | Ignore index fill factor / padding; Ignore indexes; Ignore foreign-key constraints; Ignore CHECK constraints; Ignore DML triggers |

Ignore เป็นพฤติกรรมของ comparison engine จริง: ชนิด object ที่เลือก ignore จะไม่อยู่ใน comparison/deployment selection แต่ตรวจ dependency ของ object ที่คงอยู่ใน target ต่อไป Ignore indexes ครอบคลุม ordinary indexes โดยไม่ตัด PK/UNIQUE constraints; Ignore DEFAULT constraint names ยังคงเปรียบเทียบ default expression และการ ALTER module ที่ ignore SET options จะใช้ค่า SET เดิมของ target สำหรับ module ที่มีอยู่แล้ว

Options เหล่านี้ไม่เพิ่มการรองรับ object ขั้นสูงที่ capture engine ยังไม่รองรับ และไม่ได้อ้างว่าครอบคลุมทุก Ignore option ของ Redgate

## ใช้ Source/Target ที่เปิดอยู่ใน Redgate

เมื่อเปิดโปรแกรมจะตรวจ project ที่เปิดใน Redgate โดยอัตโนมัติ หากพบ `.scp` ที่บันทึกไว้ใน `Documents\SQL Compare\SharedProjects` และตรงกับ title ของหน้าต่าง จะโหลด connection และเปิด **Edit project** ให้ตรวจแล้วกด **Compare now** ได้ทันที

กด **Import .scp** เพื่อเลือก saved project หรือโหลดค่าที่ save ใหม่ หากตรวจ project ไม่ได้ ให้เปิด Edit project ใน Redgate เพื่อให้ชื่อ `.scp` อยู่ใน title หรือเลือกไฟล์ด้วยตนเอง ค่าที่แก้ใน Redgate แต่ยังไม่ได้ save จะยังไม่อยู่ใน `.scp` จึงควรบันทึกก่อนนำเข้า

รองรับ live database connection และ SQL/Windows Authentication ตัวนำเข้ารหัสผ่านที่บันทึกไว้ใช้ `RedGate.Legacy.Serialization.dll` ของ **SQL Compare 14 ที่ติดตั้งในเครื่อง** ผ่าน PowerShell process ซึ่งส่งค่าทาง private pipe ให้ Rust เท่านั้น ไม่มีรหัสผ่านใน command line, log หรือ project JSON และไม่แจก DLL นี้ หากเครื่องไม่มี DLL หรือ project ไม่มีรหัสผ่านที่บันทึกไว้ ให้กรอกรหัสผ่านเองได้

Connection ที่นำเข้าจาก Redgate 14 เปิด **Trust server certificate** เพื่อรองรับ certificate ของ server เดิม พร้อมขอ encrypted transport เปลี่ยนได้ใน Data sources tab

**นำเข้าเฉพาะ connection settings และ IgnoreComments ที่รองรับ** — table/owner mappings, object filters และ comparison options อื่นของ Redgate ยังไม่ถูกนำมาใช้ จึงอาจได้ผลต่างจาก Redgate แม้เชื่อมต่อฐานข้อมูลเดียวกัน

## เริ่มใช้งาน

เปิด `dist\SchemaCompare-1.0.0\schema-compare.exe` แล้วกด **Edit project** เพื่อกำหนด Source และ Target

ใช้งานฐานข้อมูลจริง:

1. กด **Edit project** แล้วเลือก **Database** ที่ source และ target; source คือโครงสร้างที่ต้องการให้ target เป็น
2. กรอก server/login แล้วเปิด dropdown Database เพื่อโหลดและเลือกชื่อฐาน
3. กด **Compare now** โปรแกรมอ่าน catalog เท่านั้น ไม่แก้ฐานข้อมูล
4. เลือก object และตรวจ SQL ที่แท็บ **SQL differences**
5. ดู **Review** เพื่อแก้ blocker และอ่านขอบเขตการเปรียบเทียบ
6. กด **Deploy...** ตรวจ Target และสคริปต์ แล้วกด **Deploy to Target** เพื่อรันจากโปรแกรม หรือกด **Save SQL script** เพื่อรันภายนอก

โปรแกรม execute deployment script เฉพาะเมื่อกด Deploy to Target ค่าเริ่มต้นไม่เลือก object ที่มีเฉพาะ target หากต้องการลบต้องเลือก object และเปิด **Allow destructive drops** แยกต่างหาก

กด **Group by** เพื่อแบ่งตาม Type of object / Type of difference / No groups; checkbox ที่หัวกลุ่มเลือกทั้งกลุ่ม ค่าเริ่มต้นซ่อนรายการที่เหมือนกัน เปิด **Show identical** เพื่อดูได้ แถบคั่นระหว่าง results และ SQL Differences ลากปรับขนาดได้ SQL diff มีเลขบรรทัด, syntax colors, สีแสดงบรรทัดที่เพิ่ม/ลบ/แก้ และปุ่มเลื่อนไปความต่างถัดไป

## สิ่งที่รองรับ

| ส่วน | ความสามารถ |
|---|---|
| Input | Live SQL Server, snapshot JSON ของโปรแกรมนี้ และการจับคู่ข้ามประเภท |
| Authentication | Windows / SQL username-password; SQL password ไม่ถูกบันทึกใน project |
| Schema | dbo-owned schemas, ordinary tables, columns, identity, computed columns, defaults |
| Objects | SQL views, procedures, functions, synonyms และ enabled DML triggers |
| Index / constraint | Rowstore indexes, included columns, filters, PK, UNIQUE, FK, CHECK |
| Compare | Source only / target only / different / equal; filter; ignore comments; SQL diff |
| Migration | CREATE object, ALTER SQL module, เพิ่ม nullable column ท้ายตาราง, guarded ALTER/DROP COLUMN ตาม Source, replace index/constraint, opt-in DROP |
| Dependencies | Catalog dependencies, parent object, referenced FK table/key; topological ordering; cycle/missing-dependency blocker |
| Files | Project JSON, portable snapshots, JSON comparison report, SQL script แบบ UTF-8 BOM |
| Redgate import | `.scp` live connections, ตรวจ open project, อ่าน saved credentials เฉพาะในหน่วยความจำ |
| UI | Light theme, menus/toolbar, grouped results แบบ virtualized, resizable panes, aligned SQL diff, background capture |

## ขอบเขตและข้อจำกัดของ 1.0.0

- เปรียบเทียบ **โครงสร้าง ไม่ใช่ข้อมูลในแถว** ชื่อ object เปรียบเทียบแบบ case-sensitive และไม่ปรับ case ของ SQL/string literals; whitespace ถูกละเว้นในการเทียบ token โดยรักษาเนื้อหา string ไว้
- datatype ที่ SQL Server แปลงไม่ได้, ข้อมูลที่ไม่พอดีกับ datatype ใหม่, เปลี่ยน default/identity/computed/collation, สลับลำดับคอลัมน์ และเพิ่ม identity/computed column ยังต้อง migration เพิ่มเติม; NOT NULL ใหม่ที่ไม่มี default ต้อง backfill หาก Target มีแถวอยู่
- ยังไม่รองรับ SQL script folder, Redgate `.snp`, `.bak`, `.dacpac`, users/roles/permissions, extended properties, sequences, CLR, full-text, database options และ physical storage/filegroups
- Capture ปฏิเสธ features บางประเภทอย่างชัดเจน เช่น temporal/memory-optimized/FileTable, sparse/masked/encrypted/generated/ROWGUIDCOL/typed XML columns, user-defined column types, compressed/partitioned/disabled/special indexes, indexed views, encrypted modules และ database-level/disabled triggers
- ไม่ใช่ SQL parser แบบเต็ม และไม่ resolve dynamic SQL, cross-database/server references หรือ dependency ที่ไม่อยู่ใน catalog จึงยังต้อง review deployment script รวมถึง workload, locks, permissions และข้อมูลของ target
- ไม่ refresh schema-bound dependents ให้อัตโนมัติ; engine จะบล็อกการ DROP ที่มี retained dependents ที่ทราบอยู่ และ SQL Server อาจปฏิเสธ ALTER ที่มี dependency ซับซ้อน
- Deploy ในโปรแกรมตรวจ schema drift ก่อน execute แต่ตรวจเฉพาะขอบเขต catalog ที่รองรับ และยังมีช่วงเวลาระหว่างตรวจและรันซึ่ง session อื่นอาจแก้ schema ได้ ควรควบคุมการเปลี่ยน schema ระหว่าง deploy; สคริปต์ที่ export ไปรันเองไม่มี preflight นี้
- Script ใช้ transaction, XACT_ABORT และ `:ON ERROR EXIT`; ต้องใช้ `sqlcmd -b` หรือ SSMS SQLCMD mode เพื่อให้หยุดข้าม batch เมื่อเกิด error
- Snapshot/report มี SQL definitions จึงควรเก็บตามระดับความลับของฐานข้อมูล

## Runtime prerequisites

- Windows 10/11 x64 และ OpenGL driver สำหรับ GUI
- Microsoft **ODBC sqlcmd** และ ODBC Driver for SQL Server สำหรับ live capture; เครื่องนี้ติดตั้งอยู่แล้ว ตัวโปรแกรมหา ODBC 170 path ก่อน แล้วจึงหา `sqlcmd` ใน PATH
- SQL Server 2016 ขึ้นไปตาม catalog ที่ใช้; ทดสอบ integration กับ SQL Server 2025 LocalDB 17.0.4025.3 และอ่าน schema ของ SQL Authentication server จริงที่เปิดใน Redgate สำเร็จ ยังไม่ได้ยืนยันกับ server ทุกเวอร์ชัน
- สิทธิ์ CONNECT และ database-level VIEW DEFINITION; ไม่ต้องใช้สิทธิ์แก้ schema สำหรับ compare
- Network connections ขอ encryption โดยค่าเริ่มต้น; `Trust server certificate` ใช้กับ self-signed certificates ส่วน `(localdb)\...` ใช้ local transport ซึ่งไม่รองรับ TLS ของ sqlcmd
- ไม่ต้องติดตั้ง Rust เพื่อใช้ EXE และใช้ compare engine ได้โดยไม่ต้องมี Redgate; การอ่าน encrypted saved credentials จาก `.scp` ต้องมี SQL Compare 14 เดิมในเครื่อง

## CLI

คำสั่ง `deploy` ใช้ snapshot ที่ review ไว้และเลือก source-only/different โดยค่าเริ่มต้นเช่นเดียวกับ GUI ต้องมี native project ที่ Target เป็น live database:

```powershell
.\schema-compare-cli.exe deploy comparison.project.json source.json target.json --report verification.json
```

```powershell
.\schema-compare-cli.exe capture --server 'localhost' --database AppDev --output source.json
.\schema-compare-cli.exe capture --server 'localhost' --database AppTest --output target.json
.\schema-compare-cli.exe compare source.json target.json --report report.json --script deployment.sql
.\schema-compare-cli.exe import-redgate --active --output comparison.project.json
.\schema-compare-cli.exe import-redgate 'C:\path\comparison.scp' --compare --report report.json
.\schema-compare-cli.exe databases --server 'localhost'
.\schema-compare-cli.exe import-redgate --active --databases
```

GUI เปิด project โดยตรงได้ด้วย `schema-compare.exe --redgate-project 'C:\path\comparison.scp'` หรือ `--project comparison.project.json`; เพิ่ม `--compare` เพื่อเริ่มอ่าน schema ทันที

SQL Authentication ใช้ `--username` และ environment variable `SQLCMDPASSWORD` ของ process; ไม่รับ password ผ่าน command line และไม่บันทึกลง project เลือก `--trust-certificate` เฉพาะ connection ที่ต้องใช้

`--allow-drops` ใน CLI จะเลือก target-only objects ด้วย; ต่างจาก GUI ที่ยังต้องเลือก checkbox ของ object เอง Exit code เป็น 1 เมื่อ capture/parse/สร้าง script ล้มเหลวหรือถูกบล็อก ส่วน compare/report อย่างเดียวสำเร็จจะเป็น 0 แม้พบความต่าง

## Build และ test

เครื่องนี้เตรียม Rust 1.99.0 GNU และ portable GCC ไว้ที่ `.tools\mingw64`:

```powershell
.\build.ps1
.\build.ps1 -OutputDirectory 'dist\SchemaCompare-1.0.0'
.\test.ps1
.\test.ps1 -Integration
```

`build.ps1` ใช้ Cargo.lock และวาง EXE/sample/README ใน `dist\SchemaCompare` หรือโฟลเดอร์ที่ระบุด้วย OutputDirectory หาก EXE เดิมเปิดอยู่ ใช้โฟลเดอร์รุ่นใหม่เพื่อหลีกเลี่ยง Windows file lock ชุด 1.0.0 ที่จัดไว้ในเครื่องนี้อยู่ที่ `dist\SchemaCompare-1.0.0`

`test.ps1 -Integration` ต้องมี LocalDB; สร้าง instance ชื่อสุ่มและเก็บไฟล์ database ใน `test-results\<instance>` จากนั้น stop/delete เฉพาะ instance ที่สร้างขึ้น ผลทดสอบและไฟล์ database ยังอยู่ในโฟลเดอร์นั้นเพื่ออ่านหลักฐานได้

ย้ายไปเครื่องอื่นเพื่อ build: ติดตั้ง Rust GNU และ [WinLibs GCC x64 UCRT](https://github.com/brechtsanders/winlibs_mingw/releases) แล้วแตกให้มี `.tools\mingw64\bin\gcc.exe`; เพิ่ม bin ลง PATH และใช้ `rustup override set stable-x86_64-pc-windows-gnu` ในโฟลเดอร์ repository

หรือใช้ Rust MSVC + Visual Studio C++ Build Tools/Windows SDK ได้ด้วย:

```powershell
cargo +stable-x86_64-pc-windows-msvc build --release --target x86_64-pc-windows-msvc
```

คำสั่ง MSVC นี้ไม่ใช้ `build.ps1`; EXE อยู่ใต้ `target\x86_64-pc-windows-msvc\release` และต้องคัดลอก `samples` ไปข้าง EXE เอง

## โครงสร้างโค้ด

- `src/catalog.sql` — read-only catalog extraction และ unsupported-feature guards
- `src/catalog.rs` — sqlcmd process, authentication, lossless Unicode transport, snapshot capture
- `src/model.rs` — versioned snapshot model, validation และ identifier quoting
- `src/sql.rs` — SQL tokenizer, module header rewrite และ column DDL
- `src/engine.rs` — comparison, selection, dependency order และ migration blockers
- `src/ui.rs` — native GUI ด้วย egui/eframe
- `src/ui_editor.rs` — compact draggable project dialog, categorized Ignore options, database dropdown
- `src/database_browser.rs` — background database discovery, debounce และ stale-result rejection
- `src/project.rs` — native/Redgate project importer, active-window detection และ credential bridge
- `src/redgate_credentials.ps1` — optional bridge สำหรับ saved credentials ใน SQL Compare 14
- `src/diff_view.rs` — จัดบรรทัด source/target ให้ตรงกัน
- `src/cli.rs` — capture/compare/report/script CLI
- `tests/engine.rs`, `tests/setup.sql`, `test.ps1` — regression และ SQL Server integration tests

## หลักฐานการทดสอบ

ผ่าน Rust tests 80 รายการ และ rollback integration tests อีก 3 รายการ และ `cargo clippy --all-targets -- -D warnings` บนเครื่องนี้ รวม Ignore behavior, การรักษา SET options ของ target, dependency เมื่อ ignore FK, database discovery ที่ไม่รับ stale results และทดสอบลาก dialog/ขนาด editor หลาย frame

Integration ทดสอบ database discovery → capture → compare → generate → Deploy backend → recapture รวมการปฏิเสธ schema drift ก่อนรัน และ rollback/หยุด batch เมื่อเกิด error: catalog ที่รองรับทั้ง 17 องค์ประกอบตรงกับ source, customer row เดิมยังอยู่ และ target-only table ไม่ถูกลบ ครอบคลุม schema, computed column, filtered index, unique/FK/check, trigger, synonym, การตัด diagram tooling objects และ stored procedure ที่มี Unicode ยาว 5,000 ตัวอักษร ดู `test-results\SchemaCompare_2fadd884\verification.json`

อ่าน `.scp` ของ project ที่เปิดอยู่จริงและ reuse SQL credentials สำเร็จ: logical rows Different 15 / Source-only 3 / Target-only 10 / Identical 667 โดยอ่าน schema เท่านั้น ไม่มีการ execute deployment บน server จริง รายงานอยู่ใน `test-results\redgate-logical.local.json` ซึ่งไม่อยู่ในชุดแจกจ่าย หลัง Refresh Redgate พบ Table ที่ต่าง 15 และ Source-only 3 / Target-only Synonym 10 ตรงกัน; Redgate มี Different Role อีก 1 และ Identical 707 ซึ่งรวมชนิดที่ engine นี้ยังไม่รองรับ จึงยังไม่อ้างว่าจำนวนรวมทุกชนิดเท่ากัน

พฤติกรรม Table รวม indexes/constraints ใช้เอกสาร [Which objects can be compared?](https://documentation.red-gate.com/sc15/setting-up-the-comparison/which-objects-can-be-compared) และตรวจ SQL view ใน SQL Compare 14 ที่ติดตั้งอยู่จริง ประกอบกับ [Project options SQL Compare 14](https://documentation.red-gate.com/sc14/setting-up-the-comparison/setting-project-options)

SQLCMD runner ปิด startup script ด้วยการไม่ส่ง SQLCMDINI ไปยัง process และปฏิเสธ shell/editor commands, directives สำหรับเปลี่ยน connection/import ไฟล์หรือ GO ที่รันซ้ำในสคริปต์ Deploy ใช้ SQLCMDPASSWORD สำหรับ SQL Authentication โดยไม่ใส่ password ใน arguments

GUI smoke test 0.6.0 ใช้ LocalDB ที่แยกเฉพาะ: เห็น Source/Target connecting พร้อมกัน, Deploy สำเร็จแสดง Success แล้ว Compare เหลือ target-only 1, สร้าง schema drift แล้ว Deploy แสดง Fail พร้อม Compare ใหม่ที่มี target-only 2 และไม่ execute column addition ที่เลือกไว้ ตรวจข้อมูลเดิมยังอยู่ ดู `test-results\SchemaCompare_UI_d9995152\gui-verification.json`

ทดสอบ ALTER COLUMN int → decimal(21,6) จริงบน LocalDB แยกเฉพาะ: ค่า 123, NULL, -2147483648 และ 2147483647 คงเดิม พร้อมสร้าง PK/UNIQUE/index/check/default/FK กลับครบ และทดสอบว่ากรณี user-created statistics ทำให้ transaction rollback คืน index/FK ที่ drop ไปก่อนหน้า หลักฐาน test-results\SchemaCompare_4617a9fa\verification.json

ทดสอบ Source column authority บน LocalDB: ลด nvarchar(1000) เป็น nvarchar(500), decimal(21,6) เป็น int, ลบ Legacy/default/index และเพิ่ม Replacement/index ตาม Source โดยรักษาค่าคอลัมน์ที่คงไว้ อีกทั้งตรวจ rollback เมื่อข้อความยาว 501 ตัวอักษรหรือ decimal มีเศษที่สูญหายจากการแปลง หลักฐาน test-results\SchemaCompare_08ee8e61\verification.json


Portable build ล่าสุดอยู่ที่ dist\SchemaCompare-1.0.0-centered\schema-compare.exe และ ZIP SchemaCompare-1.0.0-win-x64.zip
# MSSQL-Schema-compare
