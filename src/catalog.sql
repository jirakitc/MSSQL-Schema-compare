SET NOCOUNT ON;
SET QUOTED_IDENTIFIER ON;
SET ANSI_NULLS ON;
IF HAS_PERMS_BY_NAME(DB_NAME(), 'DATABASE', 'VIEW DEFINITION') <> 1
    THROW 50001, 'VIEW DEFINITION permission on the database is required for a complete capture.', 1;
IF EXISTS(SELECT 1 FROM sys.objects o LEFT JOIN sys.sql_modules m ON m.object_id=o.object_id WHERE o.is_ms_shipped=0 AND o.type IN ('P','V','FN','IF','TF','TR') AND m.definition IS NULL)
    THROW 50002, 'Encrypted or inaccessible SQL modules found. Capture cannot be complete.', 1;
IF EXISTS(SELECT 1 FROM sys.tables WHERE is_ms_shipped=0 AND (temporal_type<>0 OR is_memory_optimized=1 OR is_filetable=1))
    THROW 50003, 'Temporal, memory-optimized and FileTable objects require a later engine version.', 1;
IF EXISTS(SELECT 1 FROM sys.columns c JOIN sys.tables t ON t.object_id=c.object_id WHERE t.is_ms_shipped=0 AND (c.is_sparse=1 OR c.is_column_set=1 OR c.is_filestream=1 OR c.generated_always_type<>0 OR c.encryption_type IS NOT NULL OR c.is_rowguidcol=1 OR c.xml_collection_id<>0))
    THROW 50004, 'Special column features (sparse, encrypted, generated, ROWGUIDCOL, typed XML, FILESTREAM) are not yet supported.', 1;
IF EXISTS(SELECT 1 FROM sys.columns c JOIN sys.objects o ON o.object_id=c.object_id JOIN sys.types t ON t.user_type_id=c.user_type_id WHERE o.is_ms_shipped=0 AND o.type='U' AND t.is_user_defined=1)
    THROW 50005, 'User-defined column types are not yet supported.', 1;
IF EXISTS(SELECT 1 FROM sys.indexes i JOIN sys.tables t ON t.object_id=i.object_id JOIN sys.data_spaces d ON d.data_space_id=i.data_space_id WHERE t.is_ms_shipped=0 AND (i.type NOT IN(0,1,2) OR d.type='PS' OR i.is_disabled=1))
    THROW 50006, 'Special, disabled or partitioned indexes require a later engine version.', 1;
IF EXISTS(SELECT 1 FROM sys.triggers WHERE is_ms_shipped=0 AND (parent_class<>1 OR is_disabled=1))
    THROW 50007, 'Database-level or disabled triggers are not yet supported.', 1;
IF EXISTS(SELECT 1 FROM sys.partitions p JOIN sys.tables t ON t.object_id=p.object_id WHERE t.is_ms_shipped=0 AND p.data_compression<>0)
    THROW 50008, 'Compressed tables/indexes are not yet supported.', 1;
IF EXISTS(SELECT 1 FROM sys.identity_columns c JOIN sys.tables t ON t.object_id=c.object_id WHERE t.is_ms_shipped=0 AND c.is_not_for_replication=1)
    THROW 50009, 'NOT FOR REPLICATION identity is not yet supported.', 1;
IF EXISTS(SELECT 1 FROM sys.indexes i JOIN sys.views v ON v.object_id=i.object_id WHERE v.is_ms_shipped=0)
    THROW 50011, 'Indexed views are not yet supported.', 1;
IF EXISTS(SELECT 1 FROM sys.masked_columns c JOIN sys.tables t ON t.object_id=c.object_id WHERE t.is_ms_shipped=0 AND c.is_masked=1)
    THROW 50012, 'Masked columns are not yet supported.', 1;

-- SQL Server diagram support objects are tooling infrastructure, not user schema.
CREATE TABLE #tooling_objects (object_id int PRIMARY KEY);
INSERT #tooling_objects SELECT major_id FROM sys.extended_properties
WHERE class=1 AND minor_id=0 AND name=N'microsoft_database_tools_support';
CREATE TABLE #objects ([schema] nvarchar(128), name nvarchar(128), kind varchar(20), definition nvarchar(max), parent nvarchar(128), object_id int, columns nvarchar(max), ansi_nulls bit, quoted_identifier bit);
INSERT #objects
SELECT s.name,s.name,'Schema',N'CREATE SCHEMA '+QUOTENAME(s.name)+N' AUTHORIZATION [dbo];',NULL,NULL,NULL,NULL,NULL
FROM sys.schemas s WHERE s.schema_id<16384 AND s.name NOT IN('dbo','guest','sys','INFORMATION_SCHEMA') AND s.principal_id=1;
IF EXISTS(SELECT 1 FROM sys.schemas WHERE schema_id<16384 AND name NOT IN('dbo','guest','sys','INFORMATION_SCHEMA') AND principal_id<>1)
    THROW 50010, 'Custom schema owners other than dbo are not yet supported.', 1;
INSERT #objects
SELECT s.name,t.name,'Table',N'pending',NULL,t.object_id,
 (SELECT c.name,
   QUOTENAME(ty.name)+CASE WHEN ty.name IN('varchar','char','varbinary','binary') THEN '('+CASE WHEN c.max_length=-1 THEN 'max' ELSE CONVERT(varchar(10),c.max_length) END+')'
    WHEN ty.name IN('nvarchar','nchar') THEN '('+CASE WHEN c.max_length=-1 THEN 'max' ELSE CONVERT(varchar(10),c.max_length/2) END+')'
    WHEN ty.name IN('decimal','numeric') THEN '('+CONVERT(varchar(10),c.precision)+','+CONVERT(varchar(10),c.scale)+')'
    WHEN ty.name IN('datetime2','datetimeoffset','time') THEN '('+CONVERT(varchar(10),c.scale)+')' ELSE '' END AS data_type,
   c.is_nullable AS nullable,
   CASE WHEN ic.object_id IS NOT NULL THEN CONVERT(varchar(40),ic.seed_value)+','+CONVERT(varchar(40),ic.increment_value) END AS [identity],
   cc.definition AS computed,ISNULL(cc.is_persisted,0) AS persisted,dc.definition AS [default],dc.name AS default_name,c.collation_name AS collation
  FROM sys.columns c JOIN sys.types ty ON ty.user_type_id=c.user_type_id
  LEFT JOIN sys.identity_columns ic ON ic.object_id=c.object_id AND ic.column_id=c.column_id
  LEFT JOIN sys.computed_columns cc ON cc.object_id=c.object_id AND cc.column_id=c.column_id
  LEFT JOIN sys.default_constraints dc ON dc.object_id=c.default_object_id
  WHERE c.object_id=t.object_id ORDER BY c.column_id FOR JSON PATH),NULL,NULL
FROM sys.tables t JOIN sys.schemas s ON s.schema_id=t.schema_id WHERE t.is_ms_shipped=0;
INSERT #objects
SELECT s.name,o.name,CASE o.type WHEN 'V' THEN 'View' WHEN 'P' THEN 'Procedure' WHEN 'TR' THEN 'Trigger' ELSE 'Function' END,
 m.definition,CASE WHEN o.type='TR' THEN OBJECT_NAME(o.parent_object_id) END,o.object_id,NULL,m.uses_ansi_nulls,m.uses_quoted_identifier
FROM sys.objects o JOIN sys.schemas s ON s.schema_id=o.schema_id JOIN sys.sql_modules m ON m.object_id=o.object_id
WHERE o.is_ms_shipped=0 AND o.type IN('V','P','FN','IF','TF','TR');
INSERT #objects
SELECT s.name,i.name,CASE WHEN i.is_primary_key=1 THEN 'PrimaryKey' WHEN i.is_unique_constraint=1 THEN 'Unique' ELSE 'Index' END,
 CASE WHEN i.is_primary_key=1 OR i.is_unique_constraint=1 THEN N'ALTER TABLE '+QUOTENAME(s.name)+'.'+QUOTENAME(t.name)+N' ADD CONSTRAINT '+QUOTENAME(i.name)+CASE WHEN i.is_primary_key=1 THEN ' PRIMARY KEY ' ELSE ' UNIQUE ' END
 ELSE N'CREATE '+CASE WHEN i.is_unique=1 THEN 'UNIQUE ' ELSE '' END END+
 CASE i.type WHEN 1 THEN 'CLUSTERED ' ELSE 'NONCLUSTERED ' END+
 CASE WHEN i.is_primary_key=1 OR i.is_unique_constraint=1 THEN '' ELSE 'INDEX '+QUOTENAME(i.name)+' ON '+QUOTENAME(s.name)+'.'+QUOTENAME(t.name)+' ' END+
 '('+STUFF((SELECT ','+QUOTENAME(c.name)+CASE WHEN ic.is_descending_key=1 THEN ' DESC' ELSE ' ASC' END FROM sys.index_columns ic JOIN sys.columns c ON c.object_id=ic.object_id AND c.column_id=ic.column_id WHERE ic.object_id=i.object_id AND ic.index_id=i.index_id AND ic.key_ordinal>0 ORDER BY ic.key_ordinal FOR XML PATH(''),TYPE).value('.','nvarchar(max)'),1,1,'')+')'+
 CASE WHEN EXISTS(SELECT 1 FROM sys.index_columns WHERE object_id=i.object_id AND index_id=i.index_id AND is_included_column=1) THEN ' INCLUDE ('+STUFF((SELECT ','+QUOTENAME(c.name) FROM sys.index_columns ic JOIN sys.columns c ON c.object_id=ic.object_id AND c.column_id=ic.column_id WHERE ic.object_id=i.object_id AND ic.index_id=i.index_id AND ic.is_included_column=1 ORDER BY ic.index_column_id FOR XML PATH(''),TYPE).value('.','nvarchar(max)'),1,1,'')+')' ELSE '' END+
 CASE WHEN i.has_filter=1 THEN ' WHERE '+i.filter_definition ELSE '' END+
 ' WITH (PAD_INDEX = '+CASE WHEN i.is_padded=1 THEN 'ON' ELSE 'OFF' END+CASE WHEN i.fill_factor>0 THEN ', FILLFACTOR = '+CONVERT(varchar(3),i.fill_factor) ELSE '' END+', IGNORE_DUP_KEY = '+CASE WHEN i.ignore_dup_key=1 THEN 'ON' ELSE 'OFF' END+', ALLOW_ROW_LOCKS = '+CASE WHEN i.allow_row_locks=1 THEN 'ON' ELSE 'OFF' END+', ALLOW_PAGE_LOCKS = '+CASE WHEN i.allow_page_locks=1 THEN 'ON' ELSE 'OFF' END+');',
 t.name,NULL,NULL,NULL,NULL
FROM sys.indexes i JOIN sys.tables t ON t.object_id=i.object_id JOIN sys.schemas s ON s.schema_id=t.schema_id
WHERE t.is_ms_shipped=0 AND i.index_id>0 AND i.is_hypothetical=0;
INSERT #objects
SELECT s.name,f.name,'ForeignKey',
 'ALTER TABLE '+QUOTENAME(s.name)+'.'+QUOTENAME(t.name)+CASE WHEN f.is_not_trusted=1 THEN ' WITH NOCHECK' ELSE ' WITH CHECK' END+' ADD CONSTRAINT '+QUOTENAME(f.name)+' FOREIGN KEY ('+
 STUFF((SELECT ','+QUOTENAME(COL_NAME(fc.parent_object_id,fc.parent_column_id)) FROM sys.foreign_key_columns fc WHERE fc.constraint_object_id=f.object_id ORDER BY fc.constraint_column_id FOR XML PATH(''),TYPE).value('.','nvarchar(max)'),1,1,'')+') REFERENCES '+QUOTENAME(OBJECT_SCHEMA_NAME(f.referenced_object_id))+'.'+QUOTENAME(OBJECT_NAME(f.referenced_object_id))+' ('+
 STUFF((SELECT ','+QUOTENAME(COL_NAME(fc.referenced_object_id,fc.referenced_column_id)) FROM sys.foreign_key_columns fc WHERE fc.constraint_object_id=f.object_id ORDER BY fc.constraint_column_id FOR XML PATH(''),TYPE).value('.','nvarchar(max)'),1,1,'')+') ON DELETE '+REPLACE(f.delete_referential_action_desc,'_',' ')+' ON UPDATE '+REPLACE(f.update_referential_action_desc,'_',' ')+CASE WHEN f.is_not_for_replication=1 THEN ' NOT FOR REPLICATION' ELSE '' END+';'+
 CASE WHEN f.is_disabled=1 THEN CHAR(10)+'ALTER TABLE '+QUOTENAME(s.name)+'.'+QUOTENAME(t.name)+' NOCHECK CONSTRAINT '+QUOTENAME(f.name)+';' ELSE '' END,
 t.name,f.object_id,NULL,NULL,NULL
FROM sys.foreign_keys f JOIN sys.tables t ON t.object_id=f.parent_object_id JOIN sys.schemas s ON s.schema_id=t.schema_id WHERE t.is_ms_shipped=0;
INSERT #objects
SELECT s.name,c.name,'Check',
 'ALTER TABLE '+QUOTENAME(s.name)+'.'+QUOTENAME(t.name)+CASE WHEN c.is_not_trusted=1 THEN ' WITH NOCHECK' ELSE ' WITH CHECK' END+' ADD CONSTRAINT '+QUOTENAME(c.name)+' CHECK '+CASE WHEN c.is_not_for_replication=1 THEN 'NOT FOR REPLICATION ' ELSE '' END+c.definition+';'+
 CASE WHEN c.is_disabled=1 THEN CHAR(10)+'ALTER TABLE '+QUOTENAME(s.name)+'.'+QUOTENAME(t.name)+' NOCHECK CONSTRAINT '+QUOTENAME(c.name)+';' ELSE '' END,
 t.name,c.object_id,NULL,NULL,NULL
FROM sys.check_constraints c JOIN sys.tables t ON t.object_id=c.parent_object_id JOIN sys.schemas s ON s.schema_id=t.schema_id WHERE t.is_ms_shipped=0;

-- Materialize dependency edges once: avoid scanning all object pairs in large databases.
INSERT #objects
SELECT SCHEMA_NAME(s.schema_id),s.name,'Synonym',
 N'CREATE SYNONYM '+QUOTENAME(SCHEMA_NAME(s.schema_id))+N'.'+QUOTENAME(s.name)+N' FOR '+s.base_object_name+N';',
 NULL,s.object_id,NULL,NULL,NULL FROM sys.synonyms s WHERE s.is_ms_shipped=0;
DELETE o FROM #objects o WHERE EXISTS(SELECT 1 FROM #tooling_objects t WHERE t.object_id=o.object_id)
 OR EXISTS(SELECT 1 FROM #tooling_objects t JOIN sys.objects p ON p.object_id=t.object_id
           WHERE p.name=o.parent AND SCHEMA_NAME(p.schema_id)=o.[schema]);
CREATE INDEX IX_objects_id ON #objects(object_id);
CREATE TABLE #deps(object_id int,[key] nvarchar(600));
INSERT #deps
SELECT COALESCE(dc.parent_object_id,d.referencing_id),r.kind+':'+r.[schema]+':'+ISNULL(r.parent,'')+':'+r.name
FROM sys.sql_expression_dependencies d JOIN #objects r ON r.object_id=d.referenced_id
LEFT JOIN sys.default_constraints dc ON dc.object_id=d.referencing_id
WHERE r.object_id<>COALESCE(dc.parent_object_id,d.referencing_id);
INSERT #deps
SELECT f.object_id,r.kind+':'+r.[schema]+':'+ISNULL(r.parent,'')+':'+r.name
FROM sys.foreign_keys f JOIN #objects r ON r.object_id=f.referenced_object_id AND r.kind='Table';
INSERT #deps
SELECT f.object_id,r.kind+':'+r.[schema]+':'+ISNULL(r.parent,'')+':'+r.name
FROM sys.foreign_keys f JOIN sys.indexes i ON i.object_id=f.referenced_object_id AND i.index_id=f.key_index_id
JOIN #objects r ON r.kind IN('PrimaryKey','Unique','Index') AND r.[schema]=OBJECT_SCHEMA_NAME(f.referenced_object_id) AND r.parent=OBJECT_NAME(f.referenced_object_id) AND r.name=i.name;
CREATE INDEX IX_deps_id ON #deps(object_id);
DECLARE @json nvarchar(max)=(SELECT [schema],name,kind,definition,parent,JSON_QUERY(columns) AS columns,ansi_nulls,quoted_identifier,
 JSON_QUERY((SELECT DISTINCT d.[key] FROM #deps d WHERE d.object_id=o.object_id FOR JSON PATH)) AS dep_rows
 FROM #objects o ORDER BY kind,[schema],ISNULL(parent,N''),name FOR JSON PATH);
-- Hex chunks avoid sqlcmd line wrapping, encoding loss, and truncation of long modules.
DECLARE @pos int=1;
WHILE @pos<=LEN(@json) BEGIN
 PRINT CONVERT(varchar(max),CONVERT(varbinary(max),SUBSTRING(@json,@pos,1900)),2);
 SET @pos+=1900;
END;
